//! FerrumTerm's Terminal page in the shared Settings window (font, cursor,
//! Option key, selection, shell, tabs), also shown by FerrumIDE and
//! FerrumEngineer. Keyboard and Machines are in `settings_keyboard` and
//! `settings_machines`.

use std::cell::RefCell;

use ferrum_ui::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, IconButton, IconName, StyledTypography, h_flex,
    v_flex,
};
use ferrum_ui::gpui::{
    self, AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement,
    Render, Styled, Subscription, Window, div, px,
};
use ferrum_ui::settings::SettingsPage;
use ferrum_ui::settings::widgets::{Row, Section, Segmented, Switch, select};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{TextInput, TextInputEvent};

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
        return "`fterm` was not found. Install FerrumTerm with FerrumLauncher to get it.".into();
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
                                 terminal (the sidebar then lists sessions). Also in \
                                 FerrumIDE's and FerrumEngineer's terminals.",
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
                        "Show machines",
                        Some("The machines you added (Settings › Machines), and their port forwards."),
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
            .child(crate::settings_keyboard::option_key_section(cx))
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
