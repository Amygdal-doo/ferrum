//! `fenv`: FerrumVariables from the terminal.
//!
//! - `fenv run --project shop --env dev -- npm start` runs a command with
//!   the environment's variables added to its environment. No `.env` file
//!   is written; on Unix the command replaces `fenv` (exec), so no process
//!   keeps the decrypted vault around.
//! - `fenv list` lists projects; `fenv list --project shop` lists variable
//!   names, values only with `--show`.
//! - `fenv scan` warns about stored secret values found in the project's
//!   files or git history (see `ferrumenv_core::leaks`).
//!
//! The vault is unlocked here, from the master password typed on the
//! terminal (echo off) or given on stdin with `--password-stdin`. There is
//! deliberately no socket to the running app: anything that could talk to
//! it as you could read every secret without the password.

mod password;
mod select;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, ExitCode};

use clap::{Parser, Subcommand};
use ferrumenv_core::Vault;
use ferrumenv_core::leaks::{self, Fingerprints};
use ferrumenv_core::store::{self, Session};
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(
    name = "fenv",
    version,
    about = "FerrumVariables in the terminal: run a command with a project's variables, list them."
)]
struct Cli {
    /// The vault file (default: ~/.ferrum/env/vault.json).
    #[arg(long, global = true, env = "FENV_VAULT", value_name = "PATH")]
    vault: Option<PathBuf>,
    /// Read the master password from the first line of stdin instead of
    /// asking on the terminal.
    #[arg(long, global = true)]
    password_stdin: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a command with an environment's variables set (no .env file is
    /// written): fenv run --project shop --env dev -- npm start
    Run {
        /// Project name (default: the project linked to this folder).
        #[arg(long, short)]
        project: Option<String>,
        /// Environment name or unique prefix.
        #[arg(long, short, default_value = "development")]
        env: String,
        /// The command and its arguments, after `--`.
        #[arg(required = true, last = true, value_name = "COMMAND")]
        command: Vec<OsString>,
    },
    /// List projects, or a project's variable names (values with --show).
    List {
        /// Project name (default: all projects, or the one linked to this
        /// folder when --env is given).
        #[arg(long, short)]
        project: Option<String>,
        /// Only this environment.
        #[arg(long, short)]
        env: Option<String>,
        /// Print the values too, as .env lines.
        #[arg(long)]
        show: bool,
    },
    /// Look for the project's secret values in its folder and git history
    /// (compared by hash; values are never printed). Exits 1 on findings.
    Scan {
        /// Project name (default: the project linked to this folder).
        #[arg(long, short)]
        project: Option<String>,
        /// Only the working tree, not the git history.
        #[arg(long)]
        no_history: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("fenv: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, String> {
    let vault = unlock(cli.vault, cli.password_stdin)?;
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    match cli.command {
        Command::Run {
            project,
            env,
            command,
        } => {
            let project = select::project(&vault, project.as_deref(), &cwd)?;
            let environment = select::environment(project, &env)?;
            let mut variables = Vec::with_capacity(environment.variables.len());
            for var in &environment.variables {
                if var.value.contains('\0') {
                    return Err(format!(
                        "{} contains a NUL byte and can't be passed",
                        var.key
                    ));
                }
                variables.push((var.key.clone(), Zeroizing::new(var.value.clone())));
            }
            eprintln!(
                "fenv: {} variables from {} › {}",
                variables.len(),
                project.name,
                environment.name
            );
            drop(vault);
            exec(&command, variables)
        }
        Command::List { project, env, show } => {
            if project.is_none() && env.is_none() {
                print!("{}", select::list_projects(&vault));
                return Ok(ExitCode::SUCCESS);
            }
            let project = select::project(&vault, project.as_deref(), &cwd)?;
            let envs = match &env {
                Some(name) => vec![select::environment(project, name)?],
                None => project.environments.iter().collect(),
            };
            print!("{}", select::list_variables(&envs, show));
            Ok(ExitCode::SUCCESS)
        }
        Command::Scan {
            project,
            no_history,
        } => {
            let project = select::project(&vault, project.as_deref(), &cwd)?;
            let dir = project
                .path
                .clone()
                .ok_or_else(|| format!("{} is not linked to a folder", project.name))?;
            let prints = Fingerprints::new(project);
            let name = project.name.clone();
            // Only lengths and hashes are needed from here on.
            drop(vault);
            scan(&name, &dir, &prints, !no_history)
        }
    }
}

/// Prints where the project's values were found, and what to do.
fn scan(name: &str, dir: &Path, prints: &Fingerprints, history: bool) -> Result<ExitCode, String> {
    if prints.is_empty() {
        println!(
            "{name} has no secret values of {} characters or more to look for.",
            leaks::MIN_LEN
        );
        return Ok(ExitCode::SUCCESS);
    }
    eprintln!(
        "fenv: looking for {} secret values of {name} in {}",
        prints.len(),
        dir.display()
    );
    let mut findings = leaks::scan_working_tree(dir, prints);
    if history && leaks::is_git_repo(dir) {
        findings.extend(leaks::scan_history(dir, prints).map_err(|e| e.to_string())?);
    }
    if findings.is_empty() {
        println!("No stored secret values found.");
        return Ok(ExitCode::SUCCESS);
    }
    for f in &findings {
        let place = match (f.line, &f.commit) {
            (Some(line), _) => format!("{}:{line}", f.path),
            (None, Some(commit)) => format!("{} (history, commit {commit})", f.path),
            (None, None) => f.path.clone(),
        };
        println!("{place}  {} ({})", f.label.key, f.label.environment);
    }
    println!(
        "\n{} finding(s). What to do:\n{}",
        findings.len(),
        leaks::GUIDANCE
    );
    Ok(ExitCode::FAILURE)
}

/// Reads the vault with the master password. The derived key is dropped
/// (and wiped) at once: `fenv` never writes the vault.
fn unlock(path: Option<PathBuf>, password_stdin: bool) -> Result<Vault, String> {
    let path = path
        .or_else(store::default_vault_path)
        .ok_or("no home folder: pass --vault PATH")?;
    if !path.exists() {
        return Err(format!(
            "no vault at {}: create one in FerrumVariables first",
            path.display()
        ));
    }
    let password = if password_stdin {
        password::from_stdin()?
    } else {
        password::prompt("FerrumVariables master password: ")?
    };
    let (session, vault) = Session::unlock(&path, &password).map_err(|e| e.to_string())?;
    drop(session);
    Ok(vault)
}

/// Runs `command` with `variables` added to the inherited environment.
fn exec(
    command: &[OsString],
    variables: Vec<(String, Zeroizing<String>)>,
) -> Result<ExitCode, String> {
    let (program, args) = command.split_first().ok_or("no command given")?;
    let mut process = Process::new(program);
    process.args(args);
    for (key, value) in &variables {
        process.env(key, value.as_str());
    }
    drop(variables);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Only returns on failure.
        let error = process.exec();
        Err(format!(
            "could not run {}: {error}",
            program.to_string_lossy()
        ))
    }
    #[cfg(not(unix))]
    {
        let status = process
            .status()
            .map_err(|e| format!("could not run {}: {e}", program.to_string_lossy()))?;
        Ok(status
            .code()
            .and_then(|c| u8::try_from(c).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from))
    }
}
