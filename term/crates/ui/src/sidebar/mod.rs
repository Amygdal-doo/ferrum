//! The collapsible left sidebar (⌘B): an "Agents" section with status dots
//! and a badge for agents that need you, every session's tabs and panes, and
//! a "New agent" menu (Claude Code, Codex, recent or custom commands), and
//! SSH hosts with their port forwards (see `hosts`).

mod agent_list;
pub mod agents;
mod folder_list;
mod hosts;
pub mod model;

use crate::view::TermView;
use ferrum_ui::components::{
    ButtonCommon, Clickable, ContextMenu, IconButton, IconName, IconSize, PopoverMenu,
    StyledTypography, Tooltip, h_flex, v_flex,
};
use ferrum_ui::gpui::{self, Context, DragMoveEvent, SharedString, Window, div, prelude::*, px};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{DraggedSplitter, TextInput, TextInputEvent, badge};
use ferrumterm_protocol::SessionId;
use model::Model;

impl TermView {
    pub(crate) fn render_sidebar(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let model = Model::build(&self.app.sessions, self.app.window(), &self.app.seen);

        // Top: the tabs (or every pane). Session names only matter once
        // there is more than one session.
        let mut terminals = v_flex()
            .id("sidebar-terminals")
            .flex_1()
            .min_h_0()
            .gap_0p5()
            .overflow_y_scroll()
            .pt_1()
            .pb_2();
        let settings = crate::settings::get();
        let tabs_here = settings.tab_position == crate::settings::TabPosition::Sidebar;
        let named = self.app.sessions.len() > 1;
        if model.sessions.is_empty() {
            terminals = terminals.child(placeholder("No terminals yet. Start one with +", cx));
        }
        // Every listed terminal (a tab, or a session in tabs-on-top mode) by
        // id, rendered, so folders can take theirs first.
        let mut rows: Vec<(String, SessionId, ferrum_ui::gpui::AnyElement)> = Vec::new();
        for session in &self.app.sessions {
            if tabs_here {
                for window in &session.windows {
                    let id = window.id.to_string();
                    let row = self.render_tab_row(session, window, cx).into_any_element();
                    let title = crate::view::tab_label(window, 40);
                    rows.push((id.clone(), session.id, self.folder_item(id, title, row, cx)));
                }
            } else {
                let id = session.id.to_string();
                let row = self.render_session_row(session, cx).into_any_element();
                let title = session_name(&session.name);
                rows.push((id.clone(), session.id, self.folder_item(id, title, row, cx)));
            }
        }
        let ids: Vec<String> = rows.iter().map(|(id, ..)| id.clone()).collect();
        let layout = self.app.folders.layout(&ids);
        let mut by_id: std::collections::HashMap<String, (SessionId, ferrum_ui::gpui::AnyElement)> =
            rows.into_iter()
                .map(|(id, s, row)| (id, (s, row)))
                .collect();
        for (folder, inside) in &layout.folders {
            let inside = inside
                .iter()
                .filter_map(|id| by_id.remove(id).map(|(_, row)| row))
                .collect();
            terminals = terminals.child(self.render_folder(folder, inside, cx));
        }
        if !tabs_here && named {
            // Tabs mode: the tabs run across the top, so the sidebar lists
            // the sessions (like Rex's sidebar).
            terminals = terminals.child(ferrum_ui::sidebar_label("Sessions", cx));
        }
        let mut last_session = None;
        for id in &layout.loose {
            let Some((session, row)) = by_id.remove(id) else {
                continue;
            };
            if tabs_here && named && last_session != Some(session) {
                last_session = Some(session);
                if let Some(s) = self.app.sessions.iter().find(|s| s.id == session) {
                    terminals =
                        terminals.child(ferrum_ui::sidebar_label(session_name(&s.name), cx));
                }
            }
            terminals = terminals.child(row);
        }
        // Dropping a terminal on the list's free space takes it out of its folder.
        let terminals = terminals
            .child(div().flex_1().min_h(px(24.)))
            .on_drop(
                cx.listener(|view, drag: &folder_list::DraggedTerminal, _, cx| {
                    view.move_to_folder(&drag.id, None);
                    view.after_change(cx);
                }),
            );

        // Bottom (a movable share, 30% at first): agents and SSH hosts.
        let (show_agents, show_hosts) = (settings.sidebar_agents, settings.sidebar_hosts);
        let mut bottom = v_flex()
            .id("sidebar-agents")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb_2();
        if show_agents {
            bottom = bottom.child(self.render_agents(&model, cx));
        }
        if show_hosts {
            bottom = bottom.child(self.render_hosts(cx));
        }

        let fraction = self.app.ui.agents_fraction;
        v_flex()
            .id("sidebar")
            .flex_1()
            .min_h_0()
            .w(px(width))
            .font_ui(cx)
            .text_size(px(13.))
            .text_color(colors.text)
            .on_drag_move(cx.listener(Self::resize_agents))
            .when_some(self.custom_agent.clone(), |sidebar, input| {
                sidebar.child(
                    v_flex()
                        .flex_none()
                        .p_2()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(colors.text_muted)
                                .child(self.prompt.hint()),
                        )
                        .child(input),
                )
            })
            .child(terminals)
            .when(show_agents || show_hosts, |sidebar| {
                sidebar
                    .child(div().flex_none().mx_3().h(px(1.)).bg(colors.border_variant))
                    .child(ferrum_ui::gap_splitter(
                        AGENTS_SPLITTER,
                        gpui::Axis::Vertical,
                        cx,
                    ))
                    .child(
                        v_flex()
                            .flex_none()
                            .h(gpui::relative(fraction))
                            .min_h(px(80.))
                            .child(bottom),
                    )
            })
    }

    /// A session in the sidebar (tabs mode): its current tab's mark, its
    /// name and how many tabs it has; clicking switches to it.
    fn render_session_row(
        &self,
        session: &ferrumterm_protocol::SessionInfo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = Some(session.id) == self.app.session().map(|s| s.id);
        let current = session
            .windows
            .iter()
            .find(|w| w.id == session.active_window)
            .or(session.windows.first());
        let mark = match current.and_then(crate::tab_style::TabStyle::agent_logo) {
            Some(logo) => ferrum_ui::agent_logo(logo, px(15.), cx).into_any_element(),
            None => ferrum_ui::terminal_tile(px(17.), cx).into_any_element(),
        };
        let (session_id, block) = (session.id, current.map(|w| w.focused));
        let tabs = session.windows.len();
        ferrum_ui::sidebar_item(
            SharedString::from(format!("session-row-{}", session.id)),
            active,
            cx,
        )
        .on_click(cx.listener(move |view, _, window, cx| {
            window.focus(&view.focus_handle, cx);
            if let Some(block) = block {
                view.app.focus_block(session_id, block);
            }
            view.close_overlay_sidebar();
            view.after_change(cx);
        }))
        .child(mark)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(session_name(&session.name)),
        )
        .when(tabs > 1, |row| {
            row.child(
                div()
                    .text_size(px(11.))
                    .text_color(cx.theme().colors().text_muted)
                    .child(tabs.to_string()),
            )
        })
    }

    /// A tab in the sidebar (tabs mode): chip, name, close.
    fn render_tab_row(
        &self,
        session: &ferrumterm_protocol::SessionInfo,
        window: &ferrumterm_protocol::WindowInfo,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = Some(session.id) == self.app.session().map(|s| s.id)
            && window.id == session.active_window;
        let (session_id, block) = (session.id, window.focused);
        let target = window.id.to_string();
        ferrum_ui::sidebar_item(
            SharedString::from(format!("tab-row-{}", window.id)),
            active,
            cx,
        )
        .group("tab-row")
        .pl_0p5()
        .gap_1()
        .on_click(cx.listener(move |view, _, window, cx| {
            window.focus(&view.focus_handle, cx);
            view.app.focus_block(session_id, block);
            view.close_overlay_sidebar();
            view.after_change(cx);
        }))
        .child(self.tab_chip(window, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(crate::view::tab_label(window, 40)),
        )
        .child(
            div()
                .invisible()
                .group_hover("tab-row", |s| s.visible())
                .child(
                    IconButton::new(
                        SharedString::from(format!("close-tab-{}", window.id)),
                        IconName::Close,
                    )
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Close tab"))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.app.send(ferrumterm_protocol::Request::Close {
                            target: target.clone(),
                        });
                        view.after_change(cx);
                    })),
                ),
        )
    }

    /// Drag of the divider above the Agents section.
    fn resize_agents(
        &mut self,
        event: &DragMoveEvent<DraggedSplitter>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.drag(cx).id.as_ref() != AGENTS_SPLITTER {
            return;
        }
        let bounds = event.bounds;
        let height = f32::from(bounds.size.height);
        if height <= 0. {
            return;
        }
        let from_bottom = f32::from(bounds.bottom() - event.event.position.y);
        self.app.ui.agents_fraction = crate::ui_state::clamp_agents_fraction(from_bottom / height);
        self.resizing_sidebar = true;
        cx.notify();
    }

    /// The "+" menu: a terminal, an agent here or on an SSH host, a git
    /// worktree, saved tab configurations, and a new session.
    pub(crate) fn new_menu(&self, id: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let recent = self.app.ui.recent_agents.clone();
        let tabs = self.app.ui.tab_configs.clone();
        let hosts: Vec<String> = self.app.ssh.hosts().into_iter().map(|h| h.name).collect();
        let folders = self.app.folders_enabled();
        PopoverMenu::new(id)
            .anchor(gpui::Anchor::TopRight)
            .trigger(ferrum_ui::title_button(
                SharedString::from(format!("{id}-button")),
                IconName::Plus,
                "New terminal, agent, worktree…",
            ))
            .menu(move |window, cx| {
                let weak = weak.clone();
                let recent = recent.clone();
                let tabs = tabs.clone();
                let hosts = hosts.clone();
                Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                    let on = |f: MenuAction| {
                        let weak = weak.clone();
                        move |window: &mut Window, cx: &mut gpui::App| {
                            if let Err(error) = weak.update(cx, |view, cx| {
                                window.focus(&view.focus_handle, cx);
                                f(view, window, cx);
                                view.close_overlay_sidebar();
                                view.after_change(cx);
                            }) {
                                tracing::warn!(%error, "terminal view closed before the menu ran");
                            }
                        }
                    };
                    menu = menu.icon_entry(
                        "Terminal",
                        IconName::Terminal,
                        on(Box::new(|view, _, _| view.app.new_terminal())),
                    );
                    menu = menu.header("Agent");
                    for (title, line) in agents::PRESETS {
                        menu = menu.icon_entry(
                            title,
                            IconName::Sparkle,
                            on(Box::new(move |view, _, cx| {
                                view.launch_agent(line, false, cx)
                            })),
                        );
                    }
                    for line in &recent {
                        let line = line.clone();
                        menu = menu.entry(
                            line.clone(),
                            None,
                            on(Box::new(move |view, _, cx| {
                                view.launch_agent(&line, true, cx)
                            })),
                        );
                    }
                    menu = menu.entry(
                        "Custom agent…",
                        None,
                        on(Box::new(|view, window, cx| {
                            view.begin_prompt(Prompt::CustomAgent, window, cx)
                        })),
                    );
                    if !hosts.is_empty() {
                        menu = menu.header("Cloud agent (on an SSH host)");
                        for host in &hosts {
                            for (title, line) in agents::PRESETS {
                                let host = host.clone();
                                menu = menu.entry(
                                    format!("{title} on {host}"),
                                    None,
                                    on(Box::new(move |view, _, _| {
                                        view.app.start_remote_agent(&host, line)
                                    })),
                                );
                            }
                        }
                    }
                    menu = menu.separator();
                    if folders {
                        menu = menu.icon_entry(
                            "New folder…",
                            IconName::Folder,
                            on(Box::new(|view, window, cx| {
                                view.pending_folder_item = None;
                                view.begin_prompt(Prompt::NewFolder, window, cx)
                            })),
                        );
                    }
                    for (name, command) in &tabs {
                        let (name, command) = (name.clone(), command.clone());
                        menu = menu.entry(
                            format!("New tab: {name}"),
                            None,
                            on(Box::new(move |view, _, _| {
                                view.app.start_tab_config(&name, &command)
                            })),
                        );
                    }
                    menu.icon_entry(
                        "New worktree…",
                        IconName::GitBranch,
                        on(Box::new(|view, window, cx| {
                            view.begin_prompt(Prompt::Worktree, window, cx)
                        })),
                    )
                    .icon_entry(
                        "New tab config…",
                        IconName::Plus,
                        on(Box::new(|view, window, cx| {
                            view.begin_prompt(Prompt::TabConfig, window, cx)
                        })),
                    )
                    .separator()
                    .entry(
                        "New session",
                        None,
                        on(Box::new(|view, _, _| {
                            view.app.shortcut(crate::input::Shortcut::NewSession, None)
                        })),
                    )
                }))
            })
    }

    /// In compact mode the sidebar covers the terminal: picking something
    /// in it closes it.
    pub(crate) fn close_overlay_sidebar(&mut self) {
        if self.options.compact && self.app.ui.sidebar_open {
            self.app.ui.sidebar_open = false;
        }
    }

    fn launch_agent(&mut self, command_line: &str, remember: bool, cx: &mut Context<Self>) {
        self.app.start_agent(command_line);
        self.close_overlay_sidebar();
        if remember {
            self.app.ui.remember_agent(command_line);
            self.app.save_ui();
        }
        self.after_change(cx);
    }

    /// Opens the inline field for `prompt`: Enter runs it, Escape or
    /// clicking elsewhere cancels.
    pub(crate) fn begin_prompt(
        &mut self,
        prompt: Prompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt = prompt;
        if !self.app.ui.sidebar_open {
            self.app.toggle_sidebar();
        }
        // Renaming starts from the current name; a directory from the current one.
        let text = match prompt {
            Prompt::RenameFolder(id) => self.app.folders.get(id).map(|f| f.name.clone()),
            Prompt::FolderPath(id) => self.app.folders.get(id).and_then(|f| f.path.clone()),
            _ => None,
        };
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder(prompt.placeholder())
                .monospace()
                .with_text(text.unwrap_or_default())
        });
        let subscription =
            cx.subscribe_in(
                &input,
                window,
                |view, input, event, window, cx| match event {
                    TextInputEvent::Submit => {
                        let line = input.read(cx).text().trim().to_owned();
                        view.end_custom_agent(window, cx);
                        // An empty directory clears a folder's.
                        if !line.is_empty() || matches!(view.prompt, Prompt::FolderPath(_)) {
                            view.run_prompt(&line, cx);
                        }
                    }
                    TextInputEvent::Blurred => view.end_custom_agent(window, cx),
                    TextInputEvent::Changed => {}
                },
            );
        input.update(cx, |input, cx| input.focus(window, cx));
        self.custom_agent = Some(input);
        self._custom_agent_events = Some(subscription);
        cx.notify();
    }

    fn run_prompt(&mut self, line: &str, cx: &mut Context<Self>) {
        match self.prompt {
            Prompt::CustomAgent => self.launch_agent(line, true, cx),
            Prompt::Worktree => self.app.new_worktree(line),
            Prompt::TabConfig => {
                // `name = command`; just a name opens a shell.
                let (name, command) = line.split_once('=').unwrap_or((line, ""));
                self.app.ui.save_tab_config(name, command);
                self.app.ui.save_recent_agents();
                self.app.start_tab_config(name.trim(), command);
            }
            Prompt::NewFolder => {
                let id = self.app.folders.create(line);
                if let Some(item) = self.pending_folder_item.take() {
                    self.app.folders.move_item(&item, Some(id));
                }
                self.app.folders.save();
            }
            Prompt::RenameFolder(id) => {
                self.app.folders.rename(id, line);
                self.app.folders.save();
            }
            Prompt::FolderPath(id) => {
                self.app.folders.set_path(id, line);
                self.app.folders.save();
            }
        }
        self.after_change(cx);
    }

    pub(crate) fn end_custom_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.custom_agent.take().is_some() {
            self._custom_agent_events = None;
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    /// Drag of the splitter between the sidebar and the terminal.
    pub(crate) fn resize_sidebar(
        &mut self,
        event: &DragMoveEvent<DraggedSplitter>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.drag(cx).id.as_ref() != "sidebar" {
            return;
        }
        let x = f32::from(event.event.position.x - event.bounds.left());
        self.app.ui.sidebar_width =
            x.clamp(crate::ui_state::MIN_SIDEBAR, crate::ui_state::MAX_SIDEBAR);
        self.resizing_sidebar = true;
        cx.notify();
    }

    /// Tab-bar button that shows / hides the sidebar, with the attention
    /// badge when the sidebar is hidden.
    pub(crate) fn sidebar_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.app.ui.sidebar_open;
        let attention = if open {
            0
        } else {
            Model::build(&self.app.sessions, self.app.window(), &self.app.seen).attention
        };
        h_flex()
            .flex_none()
            .gap_1()
            .child(
                ferrum_ui::sidebar_toggle("toggle-sidebar", open)
                    .tooltip(Tooltip::text(if open {
                        "Hide sidebar (⌘B)"
                    } else {
                        "Show sidebar (⌘B)"
                    }))
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.app.toggle_sidebar();
                        view.after_change(cx);
                    })),
            )
            .when(attention > 0, |toggle| {
                toggle.child(badge(
                    attention.to_string(),
                    cx.theme().status().warning,
                    cx,
                ))
            })
    }
}

/// What a "+" menu entry does to the view.
type MenuAction = Box<dyn Fn(&mut TermView, &mut Window, &mut Context<TermView>)>;

/// Drag id of the divider above the Agents section.
const AGENTS_SPLITTER: &str = "sidebar-agents";

/// What the sidebar's inline field is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Prompt {
    #[default]
    CustomAgent,
    Worktree,
    TabConfig,
    NewFolder,
    RenameFolder(u64),
    FolderPath(u64),
}

impl Prompt {
    fn placeholder(self) -> &'static str {
        match self {
            Self::CustomAgent => "Command, e.g. aider --model sonnet",
            Self::Worktree => "Branch, e.g. feature/login",
            Self::TabConfig => "name = command, e.g. dev = npm run dev",
            Self::NewFolder | Self::RenameFolder(_) => "Folder name, e.g. ferrum",
            Self::FolderPath(_) => "Directory, e.g. ~/src/ferrum (empty: none)",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::CustomAgent => "New agent: the command to run",
            Self::Worktree => "New worktree next to this repository",
            Self::TabConfig => "Save a tab config (it appears in the + menu)",
            Self::NewFolder => "New folder in the sidebar",
            Self::RenameFolder(_) => "Rename the folder",
            Self::FolderPath(_) => "Where new terminals in this folder start",
        }
    }
}

/// `3` → `Session 3`; named sessions keep their name.
fn session_name(name: &str) -> String {
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
        format!("Session {name}")
    } else {
        name.to_owned()
    }
}

pub(super) fn placeholder(text: &'static str, cx: &gpui::App) -> gpui::Div {
    div()
        .px_3()
        .py_1()
        .text_size(px(12.))
        .text_color(cx.theme().colors().text_placeholder)
        .child(text)
}
