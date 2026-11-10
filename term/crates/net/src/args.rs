//! Command lines for the system `ssh`: interactive tabs and `ssh -N` forwards.
//!
//! Saved hosts become explicit options (`-p`, `-i`, `-J`, `user@host`);
//! ssh-config hosts are just the alias, so ssh applies the user's config.
//! Destinations follow `--`, and fields are validated so nothing can be
//! smuggled in as an option.

use crate::hosts::{Host, HostSource, SavedHost};
use ferrumterm_protocol::ForwardSpec;
use std::path::Path;

/// `/usr/bin/ssh` when present (the system binary), else `ssh` from `PATH`.
#[must_use]
pub fn ssh_binary() -> String {
    ["/usr/bin/ssh", "/bin/ssh"]
        .iter()
        .find(|p| Path::new(p).exists())
        .map_or_else(|| "ssh".to_owned(), |p| (*p).to_owned())
}

fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) if !home.is_empty() => format!("{home}/{rest}"),
        _ => path.to_owned(),
    }
}

/// Options and destination for a host: `["-p", "2222", "-J", "bastion", "--", "me@box"]`.
fn destination(host: &Host) -> Result<Vec<String>, String> {
    match &host.source {
        HostSource::SshConfig(c) => {
            if c.alias.starts_with('-') {
                return Err(format!("invalid host alias “{}”", c.alias));
            }
            Ok(vec!["--".into(), c.alias.clone()])
        }
        HostSource::Saved(h) => saved_destination(h),
    }
}

fn saved_destination(h: &SavedHost) -> Result<Vec<String>, String> {
    h.validate()?;
    let mut args = Vec::new();
    if let Some(p) = h.port {
        args.extend(["-p".into(), p.to_string()]);
    }
    if let Some(i) = &h.identity_file {
        args.extend(["-i".into(), expand_home(i)]);
    }
    if let Some(j) = &h.jump_host {
        args.extend(["-J".into(), j.clone()]);
    }
    args.push("--".into());
    args.push(match &h.user {
        Some(u) => format!("{u}@{}", h.host),
        None => h.host.clone(),
    });
    Ok(args)
}

/// `ssh …` for an interactive tab; with a startup command, `ssh -t … host '<command>'`.
pub fn interactive(ssh: &str, host: &Host) -> Result<Vec<String>, String> {
    let startup = match &host.source {
        HostSource::Saved(h) => h
            .startup_command
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty()),
        HostSource::SshConfig(_) => None,
    };
    let mut argv = vec![ssh.to_owned()];
    if startup.is_some() {
        argv.push("-t".into());
    }
    argv.extend(destination(host)?);
    if let Some(cmd) = startup {
        argv.push(cmd.to_owned());
    }
    Ok(argv)
}

/// `ssh -N` for one forward. `BatchMode` keeps it from waiting on a prompt
/// nobody can see (keys must be in ssh-agent / the keychain), and
/// `ExitOnForwardFailure` makes a refused forward exit instead of idling.
pub fn forward(ssh: &str, host: &Host, spec: &ForwardSpec) -> Result<Vec<String>, String> {
    let mut argv: Vec<String> = [
        ssh,
        "-N",
        "-T",
        "-o",
        "ExitOnForwardFailure=yes",
        "-o",
        "BatchMode=yes",
        "-o",
        "ServerAliveInterval=30",
        "-o",
        "ServerAliveCountMax=3",
        spec.kind.flag(),
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    argv.push(spec.ssh_value());
    argv.extend(destination(host)?);
    Ok(argv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssh_config::ConfigHost;

    fn saved(h: SavedHost) -> Host {
        Host {
            name: h.name.clone(),
            source: HostSource::Saved(h),
        }
    }

    #[test]
    fn interactive_saved_host() {
        let h = saved(SavedHost {
            name: "box".into(),
            user: Some("me".into()),
            host: "box.example.com".into(),
            port: Some(2222),
            identity_file: Some("/keys/id".into()),
            jump_host: Some("ops@bastion".into()),
            startup_command: None,
        });
        assert_eq!(
            interactive("ssh", &h).expect("argv"),
            [
                "ssh",
                "-p",
                "2222",
                "-i",
                "/keys/id",
                "-J",
                "ops@bastion",
                "--",
                "me@box.example.com"
            ]
        );
    }

    #[test]
    fn startup_command_requests_a_tty() {
        let h = saved(SavedHost {
            name: "box".into(),
            host: "box".into(),
            startup_command: Some("tmux new -A -s main".into()),
            ..SavedHost::default()
        });
        assert_eq!(
            interactive("/usr/bin/ssh", &h).expect("argv"),
            ["/usr/bin/ssh", "-t", "--", "box", "tmux new -A -s main"]
        );
    }

    #[test]
    fn config_hosts_use_the_alias() {
        let h = Host {
            name: "pi".into(),
            source: HostSource::SshConfig(ConfigHost {
                alias: "pi".into(),
                port: Some(2222),
                ..ConfigHost::default()
            }),
        };
        assert_eq!(interactive("ssh", &h).expect("argv"), ["ssh", "--", "pi"]);
        let spec = ForwardSpec::parse("D 1080").expect("spec");
        let argv = forward("ssh", &h, &spec).expect("argv");
        assert_eq!(argv[..2], ["ssh".to_owned(), "-N".to_owned()]);
        assert!(argv.contains(&"ExitOnForwardFailure=yes".to_owned()));
        assert!(argv.contains(&"BatchMode=yes".to_owned()));
        assert_eq!(argv[argv.len() - 4..], ["-D", "1080", "--", "pi"]);
    }

    #[test]
    fn forward_for_saved_host_and_injection_refused() {
        let h = saved(SavedHost {
            name: "db".into(),
            user: Some("me".into()),
            host: "db.internal".into(),
            ..SavedHost::default()
        });
        let spec = ForwardSpec::parse("L 127.0.0.1:5432:localhost:5432").expect("spec");
        let argv = forward("ssh", &h, &spec).expect("argv");
        assert_eq!(
            argv[argv.len() - 4..],
            [
                "-L",
                "127.0.0.1:5432:localhost:5432",
                "--",
                "me@db.internal"
            ]
        );
        let evil = saved(SavedHost {
            name: "x".into(),
            host: "-oProxyCommand=touch /tmp/pwned".into(),
            ..SavedHost::default()
        });
        assert!(interactive("ssh", &evil).is_err());
        let evil_alias = Host {
            name: "-x".into(),
            source: HostSource::SshConfig(ConfigHost {
                alias: "-oProxyCommand=x".into(),
                ..ConfigHost::default()
            }),
        };
        assert!(interactive("ssh", &evil_alias).is_err());
    }
}
