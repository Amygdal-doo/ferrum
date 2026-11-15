//! Settings › Machines: this computer (is the FerrumTerm server running),
//! the machines you added with a reachability dot (a TCP connection to its
//! ssh port, refreshed every 30 s; machines behind a jump host are not
//! probed), Rename / Remove, "Add a machine" (the same dialog as the
//! sidebar's), and how many keys the SSH agent holds.
//!
//! There is no "Update": FerrumTerm runs nothing of its own on a machine,
//! it only opens `ssh` to it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use ferrum_ui::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, Icon, IconName, IconSize, Label,
    LabelCommon, LabelSize, h_flex, v_flex,
};
use ferrum_ui::gpui::{
    self, AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement,
    Render, Styled, Subscription, Window, div, px,
};
use ferrum_ui::settings::SettingsPage;
use ferrum_ui::settings::widgets::{Row, Section};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{TextInput, TextInputEvent};
use ferrumterm_net::hosts::{Host, HostSource, HostStore};

use crate::dialogs::add_machine::{AddMachine, AddMachineEvent};

/// How often reachability is checked again while the page is open.
const PROBE_EVERY: Duration = Duration::from_secs(30);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Settings › Machines.
#[derive(Default)]
pub struct MachinesPage {
    view: RefCell<Option<Entity<MachinesView>>>,
}

impl SettingsPage for MachinesPage {
    fn id(&self) -> &'static str {
        "machines"
    }
    fn title(&self) -> &'static str {
        "Machines"
    }
    fn icon(&self) -> IconName {
        IconName::Server
    }
    fn tint(&self) -> Hsla {
        gpui::rgb(0xa970ff).into()
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["ssh", "hosts", "servers", "remote", "keys", "vps"]
    }
    fn render(&self, _window: &mut Window, cx: &mut App) -> AnyElement {
        self.view
            .borrow_mut()
            .get_or_insert_with(|| cx.new(|_| MachinesView::default()))
            .clone()
            .into_any_element()
    }
}

/// Where a reachability probe connects: (host, port), or `None` when ssh
/// goes through a jump host (a direct probe would say nothing).
#[must_use]
pub fn probe_target(host: &Host) -> Option<(String, u16)> {
    match &host.source {
        HostSource::Saved(h) if h.jump_host.is_none() => {
            Some((h.host.clone(), h.port.unwrap_or(22)))
        }
        HostSource::SshConfig(c) if c.proxy_jump.is_none() => Some((
            c.hostname.clone().unwrap_or_else(|| c.alias.clone()),
            c.port.unwrap_or(22),
        )),
        _ => None,
    }
}

fn reachable(host: &str, port: u16) -> bool {
    (host, port)
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .is_some_and(|addr| TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).is_ok())
}

fn store_file() -> std::path::PathBuf {
    ferrumterm_client::paths::ssh_hosts_file()
}

/// Whether the FerrumTerm server on this Mac answers on its socket.
fn server_running() -> bool {
    std::os::unix::net::UnixStream::connect(ferrumterm_client::paths::socket()).is_ok()
}

/// Keys the SSH agent holds (`ssh-add -l`).
fn agent_key_count() -> Result<usize, String> {
    if std::env::var_os("SSH_AUTH_SOCK").is_none() {
        return Err("No SSH agent is running for this app (SSH_AUTH_SOCK is not set).".into());
    }
    let output = std::process::Command::new("ssh-add")
        .arg("-l")
        .output()
        .map_err(|e| format!("Could not run ssh-add: {e}"))?;
    parse_agent_keys(
        output.status.code(),
        &String::from_utf8_lossy(&output.stdout),
    )
}

/// Reads `ssh-add -l`: exit 0 lists one key per line (`<bits> <fingerprint>
/// <comment> (<type>)`), exit 1 means the agent has no identities, anything
/// else means the agent could not be reached.
pub fn parse_agent_keys(code: Option<i32>, stdout: &str) -> Result<usize, String> {
    match code {
        Some(0) => Ok(stdout
            .lines()
            .filter(|l| {
                l.split_whitespace()
                    .next()
                    .is_some_and(|bits| bits.parse::<u32>().is_ok())
            })
            .count()),
        Some(1) => Ok(0),
        _ => Err("Could not reach the SSH agent.".into()),
    }
}

/// What the page shows that takes I/O to find out (files, the server
/// socket, `ssh-add`), read off the UI thread.
struct Snapshot {
    machines: Result<Vec<Host>, String>,
    running: bool,
    agent_keys: Result<usize, String>,
}

impl Snapshot {
    fn read() -> Self {
        let config = ferrumterm_net::ssh_config::load_user();
        Self {
            machines: HostStore::load(&store_file()).map(|s| s.machines(&config)),
            running: server_running(),
            agent_keys: agent_key_count(),
        }
    }
}

/// How old the snapshot may get before a render reads it again.
const REFRESH_EVERY: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct MachinesView {
    /// Reachability per machine name (`None`: not probed).
    reach: HashMap<String, bool>,
    probed: Option<Instant>,
    renaming: Option<(String, Entity<TextInput>, Subscription)>,
    adding: Option<(Entity<AddMachine>, Subscription)>,
    error: Option<String>,
    snapshot: Option<Snapshot>,
    /// When the snapshot was last asked for (`None`: read it again now).
    refreshed: Option<Instant>,
}

impl MachinesView {
    /// Reads the snapshot again in the background when it is stale.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self
            .refreshed
            .is_some_and(|at| at.elapsed() < REFRESH_EVERY)
        {
            return;
        }
        self.refreshed = Some(Instant::now());
        cx.spawn(async move |this, cx| {
            let snapshot = cx
                .background_executor()
                .spawn(async { Snapshot::read() })
                .await;
            if let Err(error) = this.update(cx, |view, cx| {
                view.snapshot = Some(snapshot);
                cx.notify();
            }) {
                tracing::debug!(%error, "machines page closed before it was read");
            }
        })
        .detach();
    }

    fn probe(&mut self, machines: &[Host], cx: &mut Context<Self>) {
        if self.probed.is_some_and(|at| at.elapsed() < PROBE_EVERY) {
            return;
        }
        self.probed = Some(Instant::now());
        let targets: Vec<(String, String, u16)> = machines
            .iter()
            .filter_map(|h| probe_target(h).map(|(host, port)| (h.name.clone(), host, port)))
            .collect();
        cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move {
                    targets
                        .into_iter()
                        .map(|(name, host, port)| (name, reachable(&host, port)))
                        .collect::<Vec<_>>()
                })
                .await;
            if let Err(error) = this.update(cx, |view, cx| {
                view.reach = results.into_iter().collect();
                cx.notify();
            }) {
                tracing::debug!(%error, "machines page closed before the probe finished");
            }
        })
        .detach();
    }

    fn edit_store(
        &mut self,
        change: impl FnOnce(&mut HostStore) -> Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        let path = store_file();
        let result = HostStore::load(&path).and_then(|mut store| {
            change(&mut store)?;
            store.save(&path)
        });
        self.error = result.err();
        self.probed = None;
        self.refreshed = None;
        cx.refresh_windows();
    }

    fn start_rename(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| TextInput::new(window, cx).with_text(name.clone()));
        let from = name.clone();
        let events = cx.subscribe(&input, move |view, input, event, cx| {
            if *event == TextInputEvent::Submit {
                let to = input.read(cx).text().trim().to_owned();
                let from = from.clone();
                view.renaming = None;
                if !to.is_empty() {
                    view.edit_store(|store| store.rename_machine(&from, &to), cx);
                }
                cx.notify();
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        self.renaming = Some((name, input, events));
        cx.notify();
    }

    fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = cx.new(|cx| AddMachine::new(window, cx));
        let events = cx.subscribe(&dialog, |view, _, event, cx| match event {
            AddMachineEvent::Added(_) => {
                view.probed = None;
                view.refreshed = None;
                cx.refresh_windows();
            }
            AddMachineEvent::Dismissed => {
                view.adding = None;
                cx.notify();
            }
        });
        self.adding = Some((dialog, events));
        cx.notify();
    }

    fn machine_row(&self, i: usize, host: &Host, cx: &mut Context<Self>) -> AnyElement {
        let status = cx.theme().status().clone();
        let colors = cx.theme().colors().clone();
        let (dot, state) = match self.reach.get(&host.name) {
            Some(true) => (status.success, "Reachable"),
            Some(false) => (status.error, "Unreachable"),
            None if probe_target(host).is_none() => (colors.text_muted, "Through a jump host"),
            None => (colors.text_muted, "Checking…"),
        };
        let name = host.name.clone();
        if let Some((_, input, _)) = self.renaming.as_ref().filter(|(n, ..)| *n == host.name) {
            return h_flex()
                .w_full()
                .py_2()
                .gap_2()
                .child(div().flex_1().child(input.clone()))
                .child(
                    Button::new(("cancel-rename", i), "Cancel")
                        .style(ButtonStyle::Subtle)
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.renaming = None;
                            cx.notify();
                        })),
                )
                .into_any_element();
        }
        let rename = name.clone();
        Row::new(("machine", i), host.name.clone())
            .detail(format!("{} · {state}", host.address()))
            .icon(
                h_flex()
                    .gap_2()
                    .child(div().size(px(8.)).rounded_full().bg(dot))
                    .child(
                        Icon::new(IconName::Server)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .control(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new(("rename-machine", i), "Rename")
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.start_rename(rename.clone(), window, cx)
                            })),
                    )
                    .child(
                        Button::new(("remove-machine", i), "Remove")
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(move |view, _, _, cx| {
                                let name = name.clone();
                                view.edit_store(
                                    move |store| {
                                        store.remove_machine(&name);
                                        Ok(())
                                    },
                                    cx,
                                );
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl Render for MachinesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = cx.theme().status().clone();
        self.refresh(cx);
        let snapshot = self.snapshot.as_ref();
        let loaded = snapshot.is_some();
        let store: Result<Vec<Host>, String> =
            snapshot.map_or_else(|| Ok(Vec::new()), |s| s.machines.clone());
        let running = snapshot.is_some_and(|s| s.running);
        let agent_keys = snapshot.map(|s| s.agent_keys.clone());
        let machines = store.clone().unwrap_or_default();
        if store.is_ok() {
            self.probe(&machines, cx);
        }
        let this_computer = Section::new("This computer").row(
            Row::new(
                "this-computer",
                std::env::var("HOST")
                    .ok()
                    .filter(|h| !h.is_empty())
                    .unwrap_or_else(|| "This Mac".into()),
            )
            .detail(if running {
                "Connected: the FerrumTerm server keeps terminals running when windows close."
            } else {
                "The FerrumTerm server is not running; it starts with the next terminal."
            })
            .icon(
                h_flex()
                    .gap_2()
                    .child(div().size(px(8.)).rounded_full().bg(if running {
                        status.success
                    } else {
                        cx.theme().colors().text_muted
                    }))
                    .child(
                        Icon::new(IconName::Monitor)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    ),
            ),
        );

        let mut added = Section::new("Machines")
            .detail("Listed in the sidebar; click one there to open a terminal on it.")
            .accessory(
                Button::new("add-machine-settings", "Add a machine")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(|view, _, window, cx| view.open_add(window, cx))),
            );
        match &store {
            Ok(_) if !loaded => {}
            Ok(_) if machines.is_empty() => {
                added = added.row(
                    v_flex()
                        .w_full()
                        .py_6()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::Server).color(Color::Muted))
                        .child(Label::new("No machines yet").weight(gpui::FontWeight::SEMIBOLD))
                        .child(
                            Label::new("Add a VPS, a Mac mini or any box you reach over ssh.")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                );
            }
            Ok(_) => {
                for (i, host) in machines.iter().enumerate() {
                    added = added.row(self.machine_row(i, host, cx));
                }
            }
            Err(error) => {
                added = added.row(
                    Row::new("machines-error", "Could not read your machines")
                        .detail(error.clone()),
                );
            }
        }

        let agent_text = match agent_keys {
            Some(Ok(1)) => "The SSH agent holds 1 key.".to_owned(),
            Some(Ok(n)) => format!("The SSH agent holds {n} keys."),
            Some(Err(e)) => e,
            None => String::new(),
        };
        let ssh = Section::new("SSH").row(
            Row::new("ssh-agent", "SSH agent")
                .detail("Where ssh finds keys: the agent in SSH_AUTH_SOCK, then ~/.ssh.")
                .control(
                    Label::new(agent_text)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        );

        v_flex()
            .relative()
            .w_full()
            .gap_6()
            .child(this_computer)
            .child(added)
            .children(
                self.error
                    .clone()
                    .map(|e| Label::new(e).size(LabelSize::Small).color(Color::Error)),
            )
            .child(ssh)
            .children(self.adding.as_ref().map(|(dialog, _)| dialog.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_net::SavedHost;
    use ferrumterm_net::ssh_config::ConfigHost;

    #[test]
    fn probes_go_straight_to_the_ssh_port() {
        let saved = Host {
            name: "box".into(),
            source: HostSource::Saved(SavedHost {
                name: "box".into(),
                host: "10.0.0.5".into(),
                port: Some(2222),
                ..SavedHost::default()
            }),
        };
        assert_eq!(probe_target(&saved), Some(("10.0.0.5".into(), 2222)));
        let config = Host {
            name: "pi".into(),
            source: HostSource::SshConfig(ConfigHost {
                alias: "pi".into(),
                ..ConfigHost::default()
            }),
        };
        assert_eq!(probe_target(&config), Some(("pi".into(), 22)));
        let jumped = Host {
            name: "db".into(),
            source: HostSource::SshConfig(ConfigHost {
                alias: "db".into(),
                proxy_jump: Some("bastion".into()),
                ..ConfigHost::default()
            }),
        };
        assert_eq!(probe_target(&jumped), None);
    }

    #[test]
    fn agent_keys_come_from_the_exit_status_and_key_lines() {
        assert_eq!(
            parse_agent_keys(Some(1), "The agent has no identities.\n"),
            Ok(0)
        );
        let two = "256 SHA256:abc me@mac (ED25519)\n3072 SHA256:def work key (RSA)\n";
        assert_eq!(parse_agent_keys(Some(0), two), Ok(2));
        assert_eq!(parse_agent_keys(Some(0), ""), Ok(0));
        assert!(parse_agent_keys(Some(2), "").is_err());
        assert!(parse_agent_keys(None, "").is_err());
    }
}
