//! What the root view does: scanning, row actions, kill, port checks.
//! Blocking work (`lsof`, `ps`, `kill`, binding test sockets) runs on the
//! background executor.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::Local;
use ferrum_ui::gpui::ClipboardItem;
use ferrum_ui::prelude::*;
use ferrum_ui::project::{self, FerrumApp};
use ferrumports_core::kill::{self, KillError, Signal};
use ferrumports_core::{Listener, ListenerKey, ScanError, ScanOptions, Snapshot, remote};

use crate::app::{FerrumPorts, KillDialog, KillStage};

/// Auto-refresh interval.
pub const POLL: Duration = Duration::from_secs(2);
/// Auto-refresh interval for a remote machine (one ssh connection each).
pub const REMOTE_POLL: Duration = Duration::from_secs(10);
/// How long a signalled process gets to exit before we ask about SIGKILL.
const EXIT_WAIT: Duration = Duration::from_millis(2000);
const EXIT_CHECK: Duration = Duration::from_millis(200);

impl FerrumPorts {
    pub fn start_polling(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let alive = this.update(cx, |app, cx| {
                    let interval = if app.target.is_some() {
                        REMOTE_POLL
                    } else {
                        POLL
                    };
                    let due = app
                        .last_scan
                        .is_none_or(|at| at.elapsed() + Duration::from_millis(100) >= interval);
                    if !app.paused && due {
                        app.refresh(cx);
                    }
                    app.poll_forwards(cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    /// Starts a scan unless one is running.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.scanning {
            self.start_scan(cx);
        }
    }

    /// Starts a scan of this Mac or the remote machine; the result of any
    /// scan already running is ignored.
    pub(crate) fn start_scan(&mut self, cx: &mut Context<Self>) {
        self.scanning = true;
        self.generation += 1;
        self.last_scan = Some(Instant::now());
        let generation = self.generation;
        let options = ScanOptions { udp: self.udp };
        let host = self.target_host();
        if self.target.is_some() && host.is_none() {
            self.apply_scan(
                Err(ScanError::Remote(format!(
                    "{} is no longer in the machine list",
                    self.target.clone().unwrap_or_default()
                ))),
                cx,
            );
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match host {
                        Some(host) => remote::scan(&host, options.udp).map_err(ScanError::Remote),
                        None => ferrumports_core::scan(options),
                    }
                })
                .await;
            this.update(cx, |app, cx| {
                if app.generation == generation {
                    app.apply_scan(result, cx);
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn apply_scan(&mut self, result: Result<Snapshot, ScanError>, cx: &mut Context<Self>) {
        self.scanning = false;
        self.loaded = true;
        self.last_refresh = Some(Local::now());
        match result {
            Ok(snapshot) => {
                self.listeners = snapshot.listeners;
                self.warnings = snapshot.warnings;
                self.error = None;
                if self.selected_listener().is_none() {
                    self.selected = None;
                }
                if self.target.is_none() {
                    self.record_history();
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        cx.notify();
    }

    pub fn select(&mut self, key: ListenerKey, cx: &mut Context<Self>) {
        self.selected = (self.selected != Some(key)).then_some(key);
        cx.notify();
    }

    pub fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        self.paused = !self.paused;
        if !self.paused {
            self.refresh(cx);
        }
        cx.notify();
    }

    pub fn toggle_udp(&mut self, cx: &mut Context<Self>) {
        self.udp = !self.udp;
        self.start_scan(cx);
    }

    pub fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.notice = Some(notice);
        self.notice_task = Some(Notice::dismiss_later(cx, |app: &mut Self, _| {
            app.notice = None;
        }));
        cx.notify();
    }

    pub fn open_in_browser(&mut self, listener: &Listener, cx: &mut Context<Self>) {
        cx.open_url(&listener.url());
    }

    pub fn copy(&mut self, text: String, what: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.show_notice(Notice::info(format!("Copied {what}")), cx);
    }

    pub fn reveal(&mut self, path: &Path, cx: &mut Context<Self>) {
        cx.reveal_path(path);
    }

    pub fn open_project_in(&mut self, app: FerrumApp, folder: &Path, cx: &mut Context<Self>) {
        let notice = match project::open_in(app, folder) {
            Ok(()) => Notice::info(format!("Opening {} in {}…", folder.display(), app.name())),
            Err(e) => Notice::error(e),
        };
        self.show_notice(notice, cx);
    }

    /// Opens FerrumContainers. The shared open-in helpers
    /// (`ferrum_ui::project::FerrumApp`) cover the IDE, Git and Term only,
    /// so this uses its binary next to ours (development builds) or
    /// `open -a`. FerrumContainers takes no container argument; it opens
    /// on its container list.
    pub fn open_containers(&mut self, cx: &mut Context<Self>) {
        let spawned = match project::find_binary("ferrumdock", project::home_dir().as_deref()) {
            Some(binary) => Command::new(binary)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(drop),
            None => Command::new("open")
                .args(["-a", "FerrumContainers"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .and_then(|status| {
                    if status.success() {
                        Ok(())
                    } else {
                        Err(std::io::Error::other("not installed"))
                    }
                }),
        };
        let notice = match spawned {
            Ok(()) => Notice::info("Opening FerrumContainers…"),
            Err(e) => Notice::error(format!(
                "Could not open FerrumContainers ({e}). Install it in FerrumLauncher."
            )),
        };
        self.show_notice(notice, cx);
    }

    /// Opens the confirmation for SIGTERM.
    pub fn ask_kill(&mut self, listener: Listener, cx: &mut Context<Self>) {
        if listener.pid.is_none() {
            self.show_notice(
                Notice::error(format!(
                    "The process on port {} belongs to another user; its pid is hidden.",
                    listener.port
                )),
                cx,
            );
            return;
        }
        self.dialog = Some(KillDialog {
            listener,
            stage: KillStage::Term,
            busy: false,
            host: self.target_host(),
        });
        cx.notify();
    }

    /// Sends the dialog's signal, then waits for the process to exit.
    pub fn confirm_kill(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialog.as_mut().filter(|d| !d.busy) else {
            return;
        };
        let Some(pid) = dialog.listener.pid else {
            return;
        };
        dialog.busy = true;
        let signal = match dialog.stage {
            KillStage::Term => Signal::Term,
            KillStage::Force => Signal::Kill,
        };
        let owner = dialog.listener.user.clone();
        let host = dialog.host.clone();
        cx.spawn(async move |this, cx| {
            let target = host.clone();
            let result = cx
                .background_spawn(async move {
                    match target {
                        Some(host) => remote::send(&host, pid, signal, owner.as_deref()),
                        None => kill::send(pid, signal, owner.as_deref()),
                    }
                })
                .await;
            let mut alive = result.is_ok();
            if let Some(host) = host {
                if alive {
                    // One ssh round trip after the wait instead of polling.
                    cx.background_executor().timer(EXIT_WAIT).await;
                    alive = cx
                        .background_spawn(async move { remote::is_alive(&host, pid) })
                        .await;
                }
            } else {
                let mut waited = Duration::ZERO;
                while alive && waited < EXIT_WAIT {
                    cx.background_executor().timer(EXIT_CHECK).await;
                    waited += EXIT_CHECK;
                    alive = cx
                        .background_spawn(async move { kill::is_alive(pid) })
                        .await;
                }
            }
            this.update(cx, |app, cx| app.kill_finished(signal, result, alive, cx))
                .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn kill_finished(
        &mut self,
        signal: Signal,
        result: Result<(), KillError>,
        alive: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        let l = &dialog.listener;
        let pid = l.pid.unwrap_or_default();
        match (result, alive, signal) {
            (Err(e), _, _) => self.show_notice(Notice::error(e.to_string()), cx),
            (Ok(()), true, Signal::Term) => {
                self.dialog = Some(KillDialog {
                    stage: KillStage::Force,
                    busy: false,
                    ..dialog
                });
            }
            (Ok(()), true, Signal::Kill) => self.show_notice(
                Notice::error(format!("{} (pid {pid}) is still running.", l.process)),
                cx,
            ),
            (Ok(()), false, _) => {
                let on = dialog
                    .host
                    .as_ref()
                    .map(|h| format!(" on {}", h.name))
                    .unwrap_or_default();
                let message = format!("Stopped {} (pid {pid}) on port {}{on}", l.process, l.port);
                self.show_notice(Notice::info(message), cx);
                self.refresh(cx);
            }
        }
        cx.notify();
    }

    /// "Is port N free?" for the toolbar field.
    pub fn check_port(&mut self, cx: &mut Context<Self>) {
        let text = self.port_input.read(cx).text().trim().to_string();
        let Some(port) = text
            .trim_start_matches(':')
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
        else {
            self.show_notice(
                Notice::error(format!("“{text}” is not a port (1–65535)")),
                cx,
            );
            return;
        };
        let listeners = self.listeners.clone();
        cx.spawn(async move |this, cx| {
            let check = cx
                .background_spawn(async move { ferrumports_core::check_port(port, &listeners) })
                .await;
            this.update(cx, |app, cx| {
                let notice = if check.free {
                    Notice::info(check.message())
                } else {
                    Notice::error(check.message())
                };
                if let Some(user) = check.users.first() {
                    app.selected = Some(user.key());
                }
                app.show_notice(notice, cx);
            })
            .ok();
        })
        .detach();
    }
}
