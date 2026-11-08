//! Saved remote servers: `fterm hosts add studio tcp://100.x.y.z:7397 --token …`,
//! then `fterm -S studio ls`, `fterm -S studio attach`.

use crate::util::Ctx;
use clap::Subcommand;
use ferrumterm_client::paths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::process::ExitCode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Subcommand)]
pub enum HostsCmd {
    /// Save a remote server.
    Add {
        name: String,
        /// `tcp://100.x.y.z:7397`, a tailnet hostname, or `host:port`.
        endpoint: String,
        /// The remote server's token (`fterm server token` on that machine).
        #[arg(long)]
        token: Option<String>,
    },
    #[command(alias = "ls")]
    List,
    #[command(alias = "rm")]
    Remove { name: String },
}

fn load() -> BTreeMap<String, Host> {
    std::fs::read_to_string(paths::hosts_file())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(h: &BTreeMap<String, Host>) -> Result<(), String> {
    let path = paths::hosts_file();
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &path,
        serde_json::to_string_pretty(h).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // Contains tokens.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())
}

pub fn lookup(name: &str) -> Option<Host> {
    load().remove(name)
}

pub fn run(cmd: HostsCmd, ctx: &Ctx) -> Result<ExitCode, String> {
    let mut hosts = load();
    match cmd {
        HostsCmd::Add {
            name,
            endpoint,
            token,
        } => {
            hosts.insert(
                name.clone(),
                Host {
                    endpoint,
                    token: token.or_else(|| ctx.token.clone()),
                },
            );
            save(&hosts)?;
            println!("Saved {name}. Try: fterm -S {name} ls");
        }
        HostsCmd::List => {
            if ctx.json {
                let redacted: BTreeMap<_, _> =
                    hosts.iter().map(|(k, v)| (k, &v.endpoint)).collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&redacted).map_err(|e| e.to_string())?
                );
            } else if hosts.is_empty() {
                println!("No saved hosts.");
            } else {
                for (name, h) in hosts {
                    println!(
                        "{name}\t{}\t{}",
                        h.endpoint,
                        if h.token.is_some() {
                            "token saved"
                        } else {
                            "no token"
                        }
                    );
                }
            }
        }
        HostsCmd::Remove { name } => {
            if hosts.remove(&name).is_none() {
                return Err(format!("no host named {name}"));
            }
            save(&hosts)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}
