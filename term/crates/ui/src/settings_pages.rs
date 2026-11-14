//! FerrumTerm's own pages in the shared Settings window: Terminal (font,
//! cursor, keyboard, selection, shell, tabs) and Remote Hosts (saved SSH
//! hosts, `~/.ssh/config` hosts and the SSH agent).

use std::cell::RefCell;
use std::time::{Duration, Instant};

use ferrum_ui::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, IconButton, IconName, Label, LabelCommon,
    LabelSize, StyledTypography, h_flex, v_flex,
};
use ferrum_ui::gpui::{
    self, AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement,
    Render, Styled, Subscription, Window, div, px,
};
use ferrum_ui::settings::SettingsPage;
use ferrum_ui::settings::widgets::{Row, Section, Segmented, Switch, select};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{TextInput, TextInputEvent};
use ferrumterm_net::hosts::{HostFormFields, HostStore, SavedHost};

use crate::settings::{self, Blink, CursorStyle};

fn tint(rgb: u32) -> Hsla {
    gpui::rgb(rgb).into()
}

/// Settings › Terminal.
#[derive(Default)]
pub struct TerminalPage {
    view: RefCell<Option<Entity<TerminalSettingsView>>>,
}

impl SettingsPage for TerminalPage {
    fn id(&self) -> &'static str {
        "terminal"
    }
    fn title(&self) -> &'static str {
        "Terminal"
    }
    fn icon(&self) -> IconName {
        IconName::Terminal
    }
    fn tint(&self) -> Hsla {
        tint(0x3dd68c)
    }
    fn keywords(&self) -> &'static [&'static str] {
        &[
            "font",
            "cursor",
            "blink",
            "option",
            "meta",
            "copy",
            "selection",
            "shell",
            "startup",
        ]
    }
    fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        self.view
            .borrow_mut()
            .get_or_insert_with(|| cx.new(|cx| TerminalSettingsView::new(window, cx)))
            .clone()
            .into_any_element()
    }
}

pub struct TerminalSettingsView {
    hooks_message: Option<String>,
    font_family: Entity<TextInput>,
    startup: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
}

impl TerminalSettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current = settings::get();
        let font_family = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Lilex (bundled)")
                .with_text(current.font_family.clone().unwrap_or_default())
        });
        let startup = cx.new(|cx| {
            TextInput::new(window, cx)
                .monospace()
                .placeholder("Default login shell")
                .with_text(current.startup_command.clone())
        });
        let font_changed = cx.subscribe(&font_family, |_, input, event, cx| {
            if *event == TextInputEvent::Changed {
                let text = input.read(cx).text().trim().to_owned();
                settings::update(|s| s.font_family = (!text.is_empty()).then_some(text));
                cx.refresh_windows();
            }
        });
        let startup_saved = cx.subscribe(&startup, |_, input, event, cx| {
            if *event == TextInputEvent::Changed {
                let text = input.read(cx).text().to_owned();
                settings::update(|s| s.startup_command = text);
            }
        });
        Self {
            hooks_message: None,
            font_family,
            startup,
            _subscriptions: vec![font_changed, startup_saved],
        }
    }
}

/// `fterm` next to this executable (an app bundle ships both), else on
/// `PATH` or in `~/.ferrum/bin`.
fn fterm() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("fterm")))
        .filter(|path| path.is_file())
        .or_else(|| ferrum_ui::integrations::find_executable("fterm"))
}

fn install_fterm_skill() -> String {
    let Some(fterm) = fterm() else {
        return "`fterm` was not found. Install FerrumTerm with Ferrum Launcher to get it.".into();
    };
    match std::process::Command::new(fterm)
        .args(["skill", "setup", "--hooks"])
        .output()
    {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if text.is_empty() {
                "Installed.".into()
            } else {
                text
            }
        }
        Ok(out) => format!(
            "fterm failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(error) => format!("Could not run fterm: {error}"),
    }
}

fn switch_row(
    id: &'static str,
    title: &'static str,
    detail: Option<&'static str>,
    on: bool,
    set: fn(&mut settings::TermSettings, bool),
) -> Row {
    let mut row = Row::new(id, title).control(Switch::new(id, on).on_change(move |on, _, cx| {
        settings::update(|s| set(s, on));
        cx.refresh_windows();
    }));
    if let Some(detail) = detail {
        row = row.detail(detail);
    }
    row
}

/// A line of terminal text with the cursor drawn in the chosen style.
fn cursor_preview(style: CursorStyle, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let caret = match style {
        CursorStyle::Block => div().w(px(9.)).h(px(18.)).bg(colors.text_accent),
        CursorStyle::Beam => div().w(px(2.)).h(px(18.)).bg(colors.text_accent),
        CursorStyle::Underline => div()
            .w(px(9.))
            .h(px(18.))
            .flex()
            .items_end()
            .child(div().w_full().h(px(2.)).bg(colors.text_accent)),
    };
    h_flex()
        .w_full()
        .my_2()
        .px_3()
        .py_2()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(colors.border_variant)
        .bg(colors.terminal_background)
        .text_color(colors.terminal_foreground)
        .font_buffer(cx)
        .child("~/ferrum $")
        .child(caret)
}

impl Render for TerminalSettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = settings::get();
        let size = s.font_size;
        let style_index = CursorStyle::ALL
            .iter()
            .position(|c| *c == s.cursor_style)
            .unwrap_or(0);
        let blink_index = Blink::ALL
            .iter()
            .position(|b| *b == s.cursor_blink)
            .unwrap_or(0);
        let font_size = h_flex()
            .gap_1()
            .child(
                IconButton::new("font-smaller", IconName::Dash).on_click(move |_, _, cx| {
                    settings::update(|s| s.font_size = (s.font_size - 1.).max(8.));
                    cx.refresh_windows();
                }),
            )
            .child(
                div()
                    .w(px(48.))
                    .text_center()
                    .child(format!("{size:.0} pt")),
            )
            .child(
                IconButton::new("font-bigger", IconName::Plus).on_click(move |_, _, cx| {
                    settings::update(|s| s.font_size = (s.font_size + 1.).min(40.));
                    cx.refresh_windows();
                }),
            );

        let tab_index = usize::from(s.tab_position == settings::TabPosition::Sidebar);
        v_flex()
            .w_full()
            .gap_6()
            .child(
                Section::new("Tabs and Sidebar")
                    .row(
                        Row::new("tab-position", "Show tabs")
                            .detail(
                                "In the sidebar, under their session, or across the top of the \
                                 terminal (the sidebar then lists sessions). Also in Ferrum \
                                 Editor's and FerrumAgentic's terminals.",
                            )
                            .control(
                                Segmented::new("tab-position-segments", tab_index)
                                    .icon_item(IconName::ThreadsSidebarLeftClosed, "Top")
                                    .icon_item(IconName::ThreadsSidebarLeftOpen, "Sidebar")
                                    .on_select(|i, _, cx| {
                                        settings::update(|s| {
                                            s.tab_position = if i == 1 {
                                                settings::TabPosition::Sidebar
                                            } else {
                                                settings::TabPosition::Top
                                            }
                                        });
                                        cx.refresh_windows();
                                    }),
                            ),
                    )
                    .row(switch_row(
                        "sidebar-agents",
                        "Show agents",
                        Some("The Agents section at the bottom of the sidebar."),
                        s.sidebar_agents,
                        |s, on| s.sidebar_agents = on,
                    ))
                    .row(switch_row(
                        "sidebar-hosts",
                        "Show SSH hosts",
                        Some("Saved hosts and the hosts from ~/.ssh/config."),
                        s.sidebar_hosts,
                        |s, on| s.sidebar_hosts = on,
                    ))
                    .footnote("Click the icon before a tab's name to give it another icon and colour."),
            )
            .child(
                Section::new("Font")
                    .row(
                        Row::new("font-family", "Font family")
                            .detail("Any installed monospace font.")
                            .control(div().w(px(240.)).child(self.font_family.clone())),
                    )
                    .row(
                        Row::new("font-size", "Font size")
                            .detail("⌘+ and ⌘− zoom from here; ⌘0 returns to it.")
                            .control(font_size),
                    ),
            )
            .child(
                Section::new("Cursor")
                    .row(cursor_preview(s.cursor_style, cx))
                    .row(
                        Row::new("cursor-style", "Cursor style").control(select(
                            "cursor-style-select",
                            &CursorStyle::ALL.map(CursorStyle::label),
                            style_index,
                            |i, _, cx| {
                                settings::update(|s| s.cursor_style = CursorStyle::ALL[i]);
                                cx.refresh_windows();
                            },
                            window,
                            cx,
                        )),
                    )
                    .row(
                        Row::new("cursor-blink", "Blink speed").control(select(
                            "cursor-blink-select",
                            &Blink::ALL.map(Blink::label),
                            blink_index,
                            |i, _, cx| {
                                settings::update(|s| s.cursor_blink = Blink::ALL[i]);
                                cx.refresh_windows();
                            },
                            window,
                            cx,
                        )),
                    )
                    .row(switch_row(
                        "force-cursor",
                        "Force style",
                        Some("Ignore programs asking for another cursor shape."),
                        s.force_cursor_style,
                        |s, on| s.force_cursor_style = on,
                    )),
            )
            .child(
                Section::new("Keyboard")
                    .row(switch_row(
                        "option-meta",
                        "Use Option as Meta",
                        Some("Option+key sends Esc-prefixed keys (M-f, M-b) instead of typing é, ∑, …"),
                        s.option_as_meta,
                        |s, on| s.option_as_meta = on,
                    ))
                    .footnote("Applies to every terminal on this Mac, including the panes in Ferrum Editor and FerrumAgentic."),
            )
            .child(
                Section::new("Selection").row(switch_row(
                    "copy-on-select",
                    "Copy on selection",
                    Some("Selected text goes to the clipboard when you release the mouse."),
                    s.copy_on_select,
                    |s, on| s.copy_on_select = on,
                )),
            )
            .child(
                Section::new("Tabs").row(switch_row(
                    "activity-in-tabs",
                    "Show activity in tabs",
                    Some("A tab shows ◐ working, ⚠ blocked, ✕ error or ✓ done for its agents."),
                    s.activity_in_tabs,
                    |s, on| s.activity_in_tabs = on,
                )),
            )
            .child(
                Section::new("Agents")
                    .row(
                        Row::new("fterm-skill", "fterm skill and status hooks")
                            .detail(
                                "Teaches Claude Code and Codex to open panes with `fterm`, and \
                                 adds Claude Code hooks that report working / blocked / done.",
                            )
                            .control(
                                Button::new("install-fterm-skill", "Install")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.hooks_message = Some(install_fterm_skill());
                                        cx.notify();
                                    })),
                            ),
                    )
                    .footnote(self.hooks_message.clone().unwrap_or_else(|| {
                        "Runs `fterm skill setup --hooks`; `fterm skill remove` undoes it.".into()
                    })),
            )
            .child(
                Section::new("Shell")
                    .row(
                        Row::new("startup", "Startup command")
                            .control(div().w(px(280.)).child(self.startup.clone())),
                    )
                    .footnote(
                        "Executable and arguments; quotes group arguments. Applies to new \
                         terminals, tabs and splits. SSH hosts use their own shell.",
                    ),
            )
    }
}

/// Settings › Remote Hosts.
#[derive(Default)]
pub struct RemoteHostsPage {
    view: RefCell<Option<Entity<RemoteHostsView>>>,
}

impl SettingsPage for RemoteHostsPage {
    fn id(&self) -> &'static str {
        "remote-hosts"
    }
    fn title(&self) -> &'static str {
        "Remote Hosts"
    }
    fn icon(&self) -> IconName {
        IconName::Public
    }
    fn tint(&self) -> Hsla {
        tint(0xa970ff)
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["ssh", "servers", "agent", "keys", "remote"]
    }
    fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        self.view
            .borrow_mut()
            .get_or_insert_with(|| cx.new(|cx| RemoteHostsView::new(window, cx)))
            .clone()
            .into_any_element()
    }
}

struct HostForm {
    name: Entity<TextInput>,
    user: Entity<TextInput>,
    host: Entity<TextInput>,
    port: Entity<TextInput>,
}

pub struct RemoteHostsView {
    form: Option<HostForm>,
    error: Option<String>,
    agent_keys: Option<(Instant, Result<usize, String>)>,
}

fn store_file() -> std::path::PathBuf {
    ferrumterm_net::hosts::store_path(&ferrumterm_client::paths::state_dir())
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
    let text = String::from_utf8_lossy(&output.stdout);
    // Exit 1 with "The agent has no identities." means zero keys.
    Ok(text.lines().filter(|l| l.contains(' ')).count())
}

impl RemoteHostsView {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            form: None,
            error: None,
            agent_keys: None,
        }
    }

    fn open_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| TextInput::new(window, cx).placeholder(placeholder))
        };
        let form = HostForm {
            name: input("Name, e.g. staging", window, cx),
            user: input("User (optional)", window, cx),
            host: input("Host or IP", window, cx),
            port: input("22", window, cx),
        };
        form.name.update(cx, |i, cx| i.focus(window, cx));
        self.form = Some(form);
        self.error = None;
        cx.notify();
    }

    fn save_form(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &self.form else { return };
        let read = |e: &Entity<TextInput>| e.read(cx).text().trim().to_owned();
        let (name, user, host, port) = (
            read(&form.name),
            read(&form.user),
            read(&form.host),
            read(&form.port),
        );
        let fields = HostFormFields {
            name: &name,
            user: &user,
            host: &host,
            port: &port,
            identity_file: "",
            jump_host: "",
            startup_command: "",
        };
        let result = SavedHost::from_form(&fields).and_then(|saved| {
            let path = store_file();
            let mut store = HostStore::load(&path)?;
            store.hosts.retain(|h| h.name != saved.name);
            store.hosts.push(saved);
            store.save(&path)
        });
        match result {
            Ok(()) => {
                self.form = None;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn remove(&mut self, name: &str, cx: &mut Context<Self>) {
        let path = store_file();
        let result = HostStore::load(&path).and_then(|mut store| {
            store.hosts.retain(|h| h.name != name);
            store.save(&path)
        });
        self.error = result.err();
        cx.notify();
    }
}

impl Render for RemoteHostsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let error_color = cx.theme().status().error;
        let saved = HostStore::load(&store_file());
        let config = ferrumterm_net::ssh_config::load_user();
        if self
            .agent_keys
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() > Duration::from_secs(10))
        {
            self.agent_keys = Some((Instant::now(), agent_key_count()));
        }

        let mut hosts = Section::new("Saved Hosts").accessory(
            Button::new("add-host", "Add Remote Host…")
                .style(ButtonStyle::Subtle)
                .on_click(cx.listener(|this, _, window, cx| this.open_form(window, cx))),
        );
        match &saved {
            Ok(store) if store.hosts.is_empty() => {
                hosts = hosts.row(
                    v_flex()
                        .w_full()
                        .py_6()
                        .items_center()
                        .gap_2()
                        .child(
                            ferrum_ui::components::Icon::new(IconName::Public).color(Color::Muted),
                        )
                        .child(Label::new("No Remote Hosts").weight(gpui::FontWeight::SEMIBOLD))
                        .child(
                            Label::new("Add an SSH host to open terminals on it from the sidebar.")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                );
            }
            Ok(store) => {
                for (i, host) in store.hosts.iter().enumerate() {
                    let name = host.name.clone();
                    hosts = hosts.row(
                        Row::new(("saved-host", i), host.name.clone())
                            .detail(host.address())
                            .control(
                                IconButton::new(("remove-host", i), IconName::Trash).on_click(
                                    cx.listener(move |this, _, _, cx| this.remove(&name, cx)),
                                ),
                            ),
                    );
                }
            }
            Err(error) => {
                hosts = hosts.row(
                    Row::new("hosts-error", "Could not read saved hosts").detail(error.clone()),
                );
            }
        }

        let form = self.form.as_ref().map(|form| {
            Section::new("New Host").row(
                v_flex()
                    .py_3()
                    .gap_2()
                    .child(form.name.clone())
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().w(px(160.)).child(form.user.clone()))
                            .child(div().flex_1().child(form.host.clone()))
                            .child(div().w(px(72.)).child(form.port.clone())),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .justify_end()
                            .child(
                                Button::new("cancel-host", "Cancel")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.form = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("save-host", "Save Host")
                                    .style(ButtonStyle::Filled)
                                    .on_click(cx.listener(|this, _, _, cx| this.save_form(cx))),
                            ),
                    ),
            )
        });

        let mut from_config = Section::new("From ~/.ssh/config")
            .detail("Shown in the sidebar automatically; edit the file to change them.");
        if config.is_empty() {
            from_config = from_config.row(Row::new("no-config", "No hosts in ~/.ssh/config"));
        }
        for (i, host) in config.iter().enumerate().take(50) {
            let address = format!(
                "{}{}{}",
                host.user
                    .as_ref()
                    .map(|u| format!("{u}@"))
                    .unwrap_or_default(),
                host.hostname.as_deref().unwrap_or(&host.alias),
                host.port.map(|p| format!(":{p}")).unwrap_or_default()
            );
            from_config =
                from_config.row(Row::new(("config-host", i), host.alias.clone()).detail(address));
        }

        let agent_text = match self.agent_keys.as_ref().map(|(_, r)| r) {
            Some(Ok(1)) => "The SSH agent holds 1 key.".to_owned(),
            Some(Ok(n)) => format!("The SSH agent holds {n} keys."),
            Some(Err(e)) => e.clone(),
            None => String::new(),
        };
        let agent = Section::new("SSH").row(
            Row::new("ssh-agent", "SSH agent")
                .detail("Where ssh finds keys for remote hosts: the agent in SSH_AUTH_SOCK, then ~/.ssh.")
                .control(Label::new(agent_text).size(LabelSize::Small).color(Color::Muted)),
        );

        v_flex()
            .w_full()
            .gap_6()
            .child(hosts)
            .children(form)
            .children(
                self.error
                    .clone()
                    .map(|e| div().text_ui_sm(cx).text_color(error_color).child(e)),
            )
            .child(from_config)
            .child(agent)
    }
}
