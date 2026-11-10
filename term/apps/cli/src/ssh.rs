//! `fterm ssh <name>`: a new tab running the system `ssh` for a saved or
//! `~/.ssh/config` host; `fterm ssh --list`; `fterm ssh add|remove` for
//! FerrumTerm's own saved hosts (`ssh-hosts.json`; never `~/.ssh/config`).

use crate::util::{Ctx, passthrough_env};
use clap::{Args, Subcommand};
use ferrumterm_client::paths;
use ferrumterm_net::hosts::find_host;
use ferrumterm_net::{HostStore, SavedHost, args, ssh_config};
use ferrumterm_protocol::{Request, Response, Spawn};
use std::process::ExitCode;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct SshArgs {
    /// Host to open: a saved name or a `~/.ssh/config` alias.
    #[arg(required_unless_present = "list")]
    pub name: Option<String>,
    /// List saved and ~/.ssh/config hosts.
    #[arg(short = 'l', long)]
    pub list: bool,
    /// Session for the new tab (default: yours, or the last focused).
    #[arg(short = 's', long)]
    pub session: Option<String>,
    #[command(subcommand)]
    pub cmd: Option<SshCmd>,
}

#[derive(Subcommand)]
pub enum SshCmd {
    /// Save a host in FerrumTerm (no passwords or keys are stored).
    Add {
        name: String,
        /// `[user@]host`
        destination: String,
        #[arg(short = 'p', long)]
        port: Option<u16>,
        /// Path to a private key (ssh / ssh-agent handle its passphrase).
        #[arg(short = 'i', long)]
        identity: Option<String>,
        /// Jump host (`ssh -J`).
        #[arg(short = 'J', long)]
        jump: Option<String>,
        /// Run this instead of a login shell (`ssh -t host <command>`).
        #[arg(short = 'c', long)]
        command: Option<String>,
    },
    /// Remove a saved host (and its forwards).
    #[command(alias = "rm")]
    Remove { name: String },
}

pub fn load_store() -> Result<HostStore, String> {
    HostStore::load(&paths::ssh_hosts_file())
}

pub async fn run(a: SshArgs, ctx: &Ctx) -> Result<ExitCode, String> {
    match a.cmd {
        Some(SshCmd::Add {
            name,
            destination,
            port,
            identity,
            jump,
            command,
        }) => {
            let (user, host) = match destination.rsplit_once('@') {
                Some((u, h)) => (Some(u.to_owned()), h.to_owned()),
                None => (None, destination),
            };
            let mut store = load_store()?;
            store.upsert(
                SavedHost {
                    name: name.clone(),
                    user,
                    host,
                    port,
                    identity_file: identity,
                    jump_host: jump,
                    startup_command: command,
                },
                None,
            )?;
            store.save(&paths::ssh_hosts_file())?;
            println!("Saved {name}. Open it with: fterm ssh {name}");
            return Ok(ExitCode::SUCCESS);
        }
        Some(SshCmd::Remove { name }) => {
            let mut store = load_store()?;
            if !store.remove(&name) {
                return Err(format!(
                    "no saved host named {name} (~/.ssh/config hosts are read-only)"
                ));
            }
            store.save(&paths::ssh_hosts_file())?;
            return Ok(ExitCode::SUCCESS);
        }
        None => {}
    }
    let store = load_store()?;
    let config = ssh_config::load_user();
    if a.list {
        let hosts = store.all_hosts(&config);
        if ctx.json {
            let rows: Vec<_> = hosts
                .iter()
                .map(|h| {
                    serde_json::json!({
                        "name": h.name,
                        "address": h.address(),
                        "source": h.source_name(),
                        "forwards": store.forwards_of(&h.name).iter().map(ToString::to_string).collect::<Vec<_>>(),
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&rows).map_err(|e| e.to_string())?
            );
        } else if hosts.is_empty() {
            println!(
                "No SSH hosts. Add one with `fterm ssh add <name> user@host` or in ~/.ssh/config."
            );
        } else {
            for h in hosts {
                let forwards = store.forwards_of(&h.name).len();
                println!(
                    "{}\t{}\t{}{}",
                    h.name,
                    h.address(),
                    if h.is_saved() {
                        "saved"
                    } else {
                        "~/.ssh/config"
                    },
                    if forwards > 0 {
                        format!("\t{forwards} forward(s)")
                    } else {
                        String::new()
                    }
                );
            }
        }
        return Ok(ExitCode::SUCCESS);
    }
    let name = a.name.ok_or("which host? (fterm ssh --list)")?;
    let host = find_host(&store, &config, &name)
        .ok_or_else(|| format!("no saved or ~/.ssh/config host named {name}"))?;
    let command = args::interactive(&args::ssh_binary(), &host)?;
    let session = a.session.or_else(|| {
        ctx.is_local()
            .then(|| std::env::var("FTERM_SESSION").ok())
            .flatten()
    });
    let (c, _events) = ctx.connect().await?;
    let r = c
        .request(Request::NewWindow {
            session,
            name: Some(name),
            spawn: Spawn {
                command,
                env: passthrough_env(),
                ..Spawn::default()
            },
        })
        .await
        .map_err(|e| e.to_string())?;
    if let Response::Created {
        session,
        window,
        block,
    } = r
    {
        if ctx.json {
            println!(
                "{}",
                serde_json::json!({ "session": session.to_string(), "window": window.to_string(), "block": block.to_string() })
            );
        } else {
            println!("{session} {window} {block}");
        }
    }
    Ok(ExitCode::SUCCESS)
}
