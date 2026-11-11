//! SSH hosts, port forwards and "what's using port N?" — the app side.
//! Hosts and forward definitions live in `ssh-hosts.json` (plus read-only
//! `~/.ssh/config`); running forwards belong to the server.

use crate::app::{App, UiEvent, spawn_env};
use ferrumterm_client::paths;
use ferrumterm_net::hosts::{Host, find_host};
use ferrumterm_net::ports::{self, Listener, Signal};
use ferrumterm_net::ssh_config::{self, ConfigHost};
use ferrumterm_net::{HostStore, args};
use ferrumterm_protocol::{
    ForwardInfo, ForwardSpec, ForwardState, Request, Response, Spawn, forward_key,
};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct SshState {
    pub store: HostStore,
    pub config: Vec<ConfigHost>,
    /// What the server runs (or failed to run).
    pub forwards: Vec<ForwardInfo>,
}

impl SshState {
    #[must_use]
    pub fn hosts(&self) -> Vec<Host> {
        self.store.all_hosts(&self.config)
    }

    #[must_use]
    pub fn forward(&self, host: &str, spec: &ForwardSpec) -> Option<&ForwardInfo> {
        let key = forward_key(host, spec);
        self.forwards.iter().find(|f| f.key == key)
    }
}

/// Result of a port lookup, for the ports dialog.
#[derive(Debug, Clone)]
pub struct PortsResult {
    /// `None` = every listening port.
    pub port: Option<u16>,
    pub listeners: Result<Vec<Listener>, String>,
    pub in_use_by_other: bool,
}

/// What happened after a signal was sent.
#[derive(Debug, Clone)]
pub struct KillOutcome {
    pub pid: u32,
    pub name: String,
    pub signal: Signal,
    /// `Ok(true)` = the process is gone.
    pub result: Result<bool, String>,
}

impl App {
    /// (Re)reads saved hosts and `~/.ssh/config`.
    pub fn reload_ssh(&mut self) {
        match HostStore::load(&paths::ssh_hosts_file()) {
            Ok(store) => self.ssh.store = store,
            Err(e) => self.notify(e),
        }
        self.ssh.config = ssh_config::load_user();
    }

    pub fn save_ssh(&mut self) -> Result<(), String> {
        self.ssh.store.save(&paths::ssh_hosts_file())
    }

    pub fn refresh_forwards(&self) {
        let Some(c) = self.client.clone() else { return };
        let proxy = self.proxy.clone();
        self.rt.spawn(async move {
            if let Ok(Response::Forwards { forwards }) = c.request(Request::ForwardList).await {
                let _ = proxy.unbounded_send(UiEvent::Forwards(forwards));
            }
        });
    }

    /// New tab in the current session running `ssh` to `name`.
    pub fn open_ssh(&mut self, name: &str) {
        let Some(host) = find_host(&self.ssh.store, &self.ssh.config, name) else {
            self.notify(format!("{name} is no longer configured"));
            return;
        };
        match args::interactive(&args::ssh_binary(), &host) {
            Ok(command) => self.send(Request::NewWindow {
                session: self.session().map(|s| s.id.to_string()),
                name: Some(name.to_owned()),
                spawn: Spawn {
                    command,
                    ..spawn_env()
                },
            }),
            Err(e) => self.notify(e),
        }
    }

    /// Starts a stopped / failed forward, or stops a running one.
    pub fn toggle_forward(&mut self, host_name: &str, spec: &ForwardSpec) {
        let key = forward_key(host_name, spec);
        if self
            .ssh
            .forward(host_name, spec)
            .is_some_and(|f| f.state == ForwardState::Running)
        {
            self.send(Request::ForwardStop { key });
            return;
        }
        let Some(host) = find_host(&self.ssh.store, &self.ssh.config, host_name) else {
            return;
        };
        match args::forward(&args::ssh_binary(), &host, spec) {
            Ok(command) => {
                self.notify(format!("Starting {key}…"));
                self.send(Request::ForwardStart {
                    host: host_name.to_owned(),
                    spec: spec.clone(),
                    command,
                    env: spawn_env().env,
                });
            }
            Err(e) => self.notify(e),
        }
    }

    /// Forgets a failed forward.
    pub fn clear_forward(&self, key: String) {
        self.send(Request::ForwardStop { key });
    }

    /// Looks up `port` (or every listening port) off the UI thread.
    pub fn lookup_ports(&self, port: Option<u16>) {
        let proxy = self.proxy.clone();
        self.rt.spawn_blocking(move || {
            let result = match port {
                Some(p) => ports::lookup(p).map(|r| (r.listeners, r.in_use_by_other)),
                None => ports::listeners(None).map(|l| (l, false)),
            };
            let (listeners, in_use_by_other) = match result {
                Ok((l, other)) => (Ok(l), other),
                Err(e) => (Err(e), false),
            };
            let _ = proxy.unbounded_send(UiEvent::Ports(PortsResult {
                port,
                listeners,
                in_use_by_other,
            }));
        });
    }

    /// Sends `signal` to `pid` (only after the user confirmed) and reports
    /// whether it exited within a few seconds.
    pub fn signal_process(&self, pid: u32, name: String, signal: Signal) {
        let proxy = self.proxy.clone();
        self.rt.spawn_blocking(move || {
            let result = ports::signal(pid, signal).map(|()| {
                let deadline = Instant::now() + Duration::from_secs(3);
                while Instant::now() < deadline && ports::is_alive(pid) {
                    std::thread::sleep(Duration::from_millis(100));
                }
                !ports::is_alive(pid)
            });
            let _ = proxy.unbounded_send(UiEvent::Killed(KillOutcome {
                pid,
                name,
                signal,
                result,
            }));
        });
    }
}
