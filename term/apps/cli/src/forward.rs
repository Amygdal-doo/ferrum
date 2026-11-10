//! `fterm forward`: SSH port forwards (`-L` / `-R` / `-D`) saved per host and
//! run by the server as `ssh -N` child processes, so they outlive windows.

use crate::ssh::load_store;
use crate::util::{Ctx, passthrough_env};
use clap::Subcommand;
use ferrumterm_client::{Client, paths};
use ferrumterm_net::hosts::find_host;
use ferrumterm_net::{args, ssh_config};
use ferrumterm_protocol::{ForwardInfo, ForwardSpec, ForwardState, Request, Response, forward_key};
use std::process::ExitCode;

#[derive(Subcommand)]
pub enum ForwardCmd {
    /// Saved forwards and what the server is running.
    #[command(alias = "ls")]
    List,
    /// Save a forward for a host: `L 8080:localhost:80`, `R 9000:localhost:3000`, `D 1080`.
    Add {
        host: String,
        #[arg(required = true, num_args = 1..)]
        spec: Vec<String>,
    },
    /// Delete a saved forward (`L8080` or its spec).
    #[command(alias = "rm")]
    Remove { host: String, id: String },
    /// Start `host/L8080`, every saved forward of `host`, or `host <spec>` once.
    Start {
        target: String,
        /// One-off spec instead of the saved ones (not saved).
        spec: Vec<String>,
    },
    /// Stop `host/L8080`, all forwards of `host`, or `--all`.
    Stop {
        #[arg(required_unless_present = "all")]
        target: Option<String>,
        #[arg(long)]
        all: bool,
    },
}

async fn running(c: &Client) -> Result<Vec<ForwardInfo>, String> {
    match c
        .request(Request::ForwardList)
        .await
        .map_err(|e| e.to_string())?
    {
        Response::Forwards { forwards } => Ok(forwards),
        _ => Err("unexpected reply".into()),
    }
}

fn state_text(f: Option<&ForwardInfo>) -> String {
    match f {
        None => "stopped".into(),
        Some(f) if f.state == ForwardState::Running => {
            format!(
                "running (pid {})",
                f.pid.map_or("?".into(), |p| p.to_string())
            )
        }
        Some(f) => format!("failed: {}", f.message.as_deref().unwrap_or("")),
    }
}

pub async fn run(cmd: ForwardCmd, ctx: &Ctx) -> Result<ExitCode, String> {
    let mut store = load_store()?;
    let config = ssh_config::load_user();
    match cmd {
        ForwardCmd::Add { host, spec } => {
            if find_host(&store, &config, &host).is_none() {
                return Err(format!("no saved or ~/.ssh/config host named {host}"));
            }
            let spec = ForwardSpec::parse(&spec.join(" "))?;
            println!("Saved {}", forward_key(&host, &spec));
            store.add_forward(&host, spec);
            store.save(&paths::ssh_hosts_file())?;
        }
        ForwardCmd::Remove { host, id } => {
            if !store.remove_forward(&host, &id) {
                return Err(format!("{host} has no saved forward {id}"));
            }
            store.save(&paths::ssh_hosts_file())?;
        }
        ForwardCmd::List => {
            let (c, _events) = ctx.connect().await?;
            let live = running(&c).await?;
            let mut rows: Vec<(String, String, ForwardSpec, Option<ForwardInfo>)> = Vec::new();
            for (host, specs) in &store.forwards {
                for spec in specs {
                    let key = forward_key(host, spec);
                    let info = live.iter().find(|f| f.key == key).cloned();
                    rows.push((key, host.clone(), spec.clone(), info));
                }
            }
            for f in &live {
                if !rows.iter().any(|r| r.0 == f.key) {
                    rows.push((
                        f.key.clone(),
                        f.host.clone(),
                        f.spec.clone(),
                        Some(f.clone()),
                    ));
                }
            }
            if ctx.json {
                let out: Vec<_> = rows
                    .iter()
                    .map(|(key, host, spec, info)| {
                        serde_json::json!({
                            "key": key,
                            "host": host,
                            "spec": spec.to_string(),
                            "state": info.as_ref().map_or("stopped", |i| match i.state {
                                ForwardState::Running => "running",
                                ForwardState::Failed => "failed",
                            }),
                            "pid": info.as_ref().and_then(|i| i.pid),
                            "message": info.as_ref().and_then(|i| i.message.clone()),
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
                );
            } else if rows.is_empty() {
                println!("No forwards. Save one: fterm forward add <host> L 8080:localhost:80");
            } else {
                for (key, _, spec, info) in &rows {
                    println!("{key}\t{}\t{}", spec.describe(), state_text(info.as_ref()));
                }
            }
        }
        ForwardCmd::Start { target, spec } => {
            let (host_name, id) = match target.split_once('/') {
                Some((h, i)) => (h.to_owned(), Some(i.to_owned())),
                None => (target.clone(), None),
            };
            let host = find_host(&store, &config, &host_name)
                .ok_or_else(|| format!("no saved or ~/.ssh/config host named {host_name}"))?;
            let specs: Vec<ForwardSpec> = if !spec.is_empty() {
                vec![ForwardSpec::parse(&spec.join(" "))?]
            } else {
                let saved = store.forwards_of(&host_name);
                let picked: Vec<_> = saved
                    .iter()
                    .filter(|s| id.as_deref().is_none_or(|i| s.id().eq_ignore_ascii_case(i)))
                    .cloned()
                    .collect();
                if picked.is_empty() {
                    return Err(match id {
                        Some(i) => format!("{host_name} has no saved forward {i}"),
                        None => format!(
                            "{host_name} has no saved forwards (fterm forward add {host_name} L 8080:localhost:80)"
                        ),
                    });
                }
                picked
            };
            let (c, _events) = ctx.connect().await?;
            let ssh = args::ssh_binary();
            let mut failed = false;
            for spec in specs {
                let command = args::forward(&ssh, &host, &spec)?;
                let key = forward_key(&host_name, &spec);
                match c
                    .request(Request::ForwardStart {
                        host: host_name.clone(),
                        spec: spec.clone(),
                        command,
                        env: passthrough_env(),
                    })
                    .await
                {
                    Ok(Response::Forwards { forwards }) => {
                        let pid = forwards.first().and_then(|f| f.pid);
                        println!(
                            "{key}\t{}\trunning (pid {})",
                            spec.describe(),
                            pid.map_or("?".into(), |p| p.to_string())
                        );
                    }
                    Ok(_) => println!("{key}\trunning"),
                    Err(e) => {
                        failed = true;
                        eprintln!("{key}\t{e}");
                    }
                }
            }
            if failed {
                return Ok(ExitCode::FAILURE);
            }
        }
        ForwardCmd::Stop { target, all } => {
            let (c, _events) = ctx.connect().await?;
            let live = running(&c).await?;
            let keys: Vec<String> = live
                .iter()
                .filter(|f| {
                    all || target
                        .as_deref()
                        .is_some_and(|t| f.key == t || f.host == t || f.key.eq_ignore_ascii_case(t))
                })
                .map(|f| f.key.clone())
                .collect();
            if keys.is_empty() {
                return Err(format!(
                    "no forward matches {}",
                    target.as_deref().unwrap_or("--all")
                ));
            }
            for key in keys {
                c.request(Request::ForwardStop { key: key.clone() })
                    .await
                    .map_err(|e| e.to_string())?;
                println!("stopped {key}");
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}
