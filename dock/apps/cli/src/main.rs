//! `fec`: FerrumContainers from the terminal, like `colima` and `docker` in
//! one. It starts and stops the VM the engine runs in (Colima, OrbStack,
//! Docker Desktop or Podman), runs `docker compose up` / `down`, opens the
//! app, and hands every other command to `docker`, pointed at that VM.

use ferrumdock_engine::DockerCli;
use ferrumdock_engine::vm::{self, Vm, VmSettings};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const HELP: &str = "\
fec — FerrumContainers in the terminal

  fec                      open FerrumContainers
  fec <folder>             open FerrumContainers on a project folder (fec .)
  fec start [args]         start the VM (args go to its CLI, e.g. --cpu 4)
  fec stop [args]          stop the VM and everything in it
  fec restart              stop, then start the VM
  fec status               which VM, whether it runs, running containers
  fec vm [name|auto]       show or choose the VM: colima, orbstack, desktop, podman
  fec autostop [min|off]   stop the VM after this many idle minutes (app open)
  fec up [args]            start the VM if needed, then docker compose up -d
  fec down [args]          docker compose down
  fec <docker command>     anything else runs docker: fec ps, fec logs -f web,
                           fec exec -it web sh, fec start web, fec compose ps …

  --vm <name>              before the command: use this VM once
";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut settings = VmSettings::load();
    let mut chosen = settings.vm;
    if args.first().is_some_and(|a| a == "--vm") {
        let Some(name) = args.get(1).cloned() else {
            return fail("--vm needs a name: colima, orbstack, desktop or podman");
        };
        let Some(vm) = Vm::parse(&name) else {
            return fail(&format!("unknown VM `{name}`"));
        };
        chosen = Some(vm);
        args.drain(..2);
    }
    let Some(command) = args.first().cloned() else {
        return open_app(None);
    };
    let rest = &args[1..];
    // `fec start` alone is the VM; `fec start web` is a container.
    let only_flags = rest.iter().all(|a| a.starts_with('-'));
    match command.as_str() {
        "help" | "-h" | "--help" => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        "start" if only_flags => with_vm(chosen, |vm| start(vm, rest)),
        "stop" if only_flags => with_vm(chosen, |vm| stop(vm, rest)),
        "restart" if only_flags => with_vm(chosen, |vm| {
            stop(vm, &[])?;
            start(vm, rest)
        }),
        "status" => status(chosen),
        "vm" => match rest.first() {
            None => {
                show_vms(chosen);
                ExitCode::SUCCESS
            }
            Some(name) if name == "auto" => {
                settings.vm = None;
                save(&settings, "fec now picks the VM itself")
            }
            Some(name) => match Vm::parse(name) {
                Some(vm) => {
                    settings.vm = Some(vm);
                    save(&settings, &format!("fec now uses {}", vm.name()))
                }
                None => fail(&format!("unknown VM `{name}`")),
            },
        },
        "autostop" => match rest.first().map(String::as_str) {
            None => {
                println!("{}", autostop_text(settings.auto_stop_minutes));
                ExitCode::SUCCESS
            }
            Some("off" | "0") => {
                settings.auto_stop_minutes = 0;
                save(&settings, &autostop_text(0))
            }
            Some(minutes) => match minutes.parse::<u32>() {
                Ok(m) => {
                    settings.auto_stop_minutes = m;
                    save(&settings, &autostop_text(m))
                }
                Err(_) => fail("autostop takes minutes or `off`"),
            },
        },
        "up" => {
            if let Some(vm) = vm::pick(chosen)
                && !vm.is_running()
                && let Err(code) = start(vm, &[])
            {
                return code;
            }
            let mut compose = vec!["compose".to_owned(), "up".to_owned()];
            if !rest.iter().any(|a| a == "-d" || a == "--detach") {
                compose.push("-d".into());
            }
            compose.extend(rest.iter().cloned());
            docker(chosen, &compose)
        }
        "down" => {
            let mut compose = vec!["compose".to_owned(), "down".to_owned()];
            compose.extend(rest.iter().cloned());
            docker(chosen, &compose)
        }
        _ if args.len() == 1 && Path::new(&command).is_dir() => open_app(Some(&command)),
        _ => docker(chosen, &args),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("fec: {message}");
    ExitCode::FAILURE
}

fn save(settings: &VmSettings, done: &str) -> ExitCode {
    match settings.save() {
        Ok(()) => {
            println!("{done}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("could not save the setting: {e}")),
    }
}

fn autostop_text(minutes: u32) -> String {
    if minutes == 0 {
        "Auto-stop is off".to_owned()
    } else {
        format!(
            "The VM stops after {minutes} min with no running container (while FerrumContainers is open)"
        )
    }
}

/// Runs `f` on the VM to use, or explains that none is installed.
fn with_vm(chosen: Option<Vm>, f: impl FnOnce(Vm) -> Result<(), ExitCode>) -> ExitCode {
    match vm::pick(chosen) {
        Some(vm) => f(vm).err().unwrap_or(ExitCode::SUCCESS),
        None => fail("no VM found: install Colima, OrbStack, Docker Desktop or Podman"),
    }
}

fn start(vm: Vm, extra: &[String]) -> Result<(), ExitCode> {
    if vm.is_running() && extra.is_empty() {
        println!("{} is already running", vm.name());
        return Ok(());
    }
    println!("Starting {}…", vm.name());
    vm.start(extra)
        .map(|()| println!("{} is running", vm.name()))
        .map_err(|e| fail(&format!("could not start {}: {e}", vm.name())))
}

fn stop(vm: Vm, extra: &[String]) -> Result<(), ExitCode> {
    if !vm.is_running() {
        println!("{} is not running", vm.name());
        return Ok(());
    }
    println!("Stopping {}…", vm.name());
    vm.stop(extra)
        .map(|()| println!("{} stopped", vm.name()))
        .map_err(|e| fail(&format!("could not stop {}: {e}", vm.name())))
}

fn show_vms(chosen: Option<Vm>) {
    let installed = vm::installed();
    if installed.is_empty() {
        println!("No VM installed (Colima, OrbStack, Docker Desktop or Podman).");
        return;
    }
    let picked = vm::pick(chosen);
    for vm in installed {
        let mark = if Some(vm) == picked { "*" } else { " " };
        let state = if vm.is_running() {
            "running"
        } else {
            "stopped"
        };
        println!("{mark} {:<9} {:<15} {state}", vm.key(), vm.name());
    }
    if chosen.is_none() {
        println!("\n(picked automatically; choose one with `fec vm <name>`)");
    }
}

fn status(chosen: Option<Vm>) -> ExitCode {
    let Some(vm) = vm::pick(chosen) else {
        return fail("no VM found: install Colima, OrbStack, Docker Desktop or Podman");
    };
    let running = vm.is_running();
    println!(
        "{}: {}",
        vm.name(),
        if running { "running" } else { "stopped" }
    );
    println!("{}", autostop_text(VmSettings::load().auto_stop_minutes));
    if running {
        let host = docker_host(vm);
        if let Some(cli) = DockerCli::detect() {
            let mut command = Command::new(&cli.path);
            command
                .args(["ps", "--format", "  {{.Names}}\t{{.Status}}"])
                .env("PATH", cli.child_path());
            if let Some(host) = &host {
                command.env("DOCKER_HOST", host);
            }
            if let Ok(out) = command.output() {
                let list = String::from_utf8_lossy(&out.stdout);
                let count = list.lines().count();
                println!(
                    "{count} running container{}",
                    if count == 1 { "" } else { "s" }
                );
                print!("{list}");
            }
        }
    }
    ExitCode::SUCCESS
}

/// `DOCKER_HOST` for `vm` when its socket is fixed and no host is set.
fn docker_host(vm: Vm) -> Option<String> {
    if std::env::var_os("DOCKER_HOST").is_some() {
        return None;
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    vm.socket(&home)
        .filter(|s| s.exists())
        .map(|s| format!("unix://{}", s.display()))
}

/// Runs `docker <args>` against the VM's engine, passing the exit code on.
fn docker(chosen: Option<Vm>, args: &[String]) -> ExitCode {
    let Some(cli) = DockerCli::detect() else {
        return fail("docker is not installed (fec runs docker for this command)");
    };
    let mut command = Command::new(&cli.path);
    command.args(args).env("PATH", cli.child_path());
    if let Some(host) = vm::pick(chosen).and_then(docker_host) {
        command.env("DOCKER_HOST", host);
    }
    match command.status() {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8),
        Err(e) => fail(&format!("could not run docker: {e}")),
    }
}

/// Opens the app, on `folder` if given: the `ferrumdock` next to this
/// binary (in the app bundle), else the installed app.
fn open_app(folder: Option<&str>) -> ExitCode {
    let folder = folder.map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| PathBuf::from(f)));
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|exe| std::fs::canonicalize(exe).ok())
        .and_then(|exe| exe.parent().map(|dir| dir.join("ferrumdock")))
        .filter(|p| p.is_file());
    let result = match sibling {
        Some(app) => {
            let mut command = Command::new(app);
            command.args(folder.iter());
            command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map(|_| ())
        }
        None => {
            let mut command = Command::new("/usr/bin/open");
            command.args(["-a", "FerrumContainers"]);
            if let Some(folder) = &folder {
                command.arg("--args").arg(folder);
            }
            command.status().map(|_| ())
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&format!("could not open FerrumContainers: {e}")),
    }
}
