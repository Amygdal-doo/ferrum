//! The window chrome: the native title bar is hidden, the file tree is the
//! sidebar (title row with traffic lights and ◧ ⌘ +, then the inline
//! project bar), and the editor card's header is the main title row (its
//! tabs, and the [Git] [Agent] [Term] toggles at its right end). While the
//! sidebar is hidden, ◧ ⌘ + and the project move into that header. Each
//! toggle opens or closes its tool as a pane inside the window;
//! right-clicking Git or Term offers the stand-alone app instead.

use ferrum_ui::components::{Clickable, ContextMenu, IconName, Toggleable, right_click_menu};
use ferrum_ui::prelude::*;
use ferrumeditor_core::keymap::display_keys;
use ferrumeditor_core::tool_layout::ToolKind;

use super::Workspace;
use super::tools::{external_app, tool_icon};
use crate::settings::{MAC, Settings};

/// The top-bar tools, with the command whose shortcut toggles each.
pub const TOP_BAR_TOOLS: [(ToolKind, &str); 3] = [
    (ToolKind::Git, "git:toggle-view"),
    (ToolKind::Agent, "agents:toggle-panel"),
    (ToolKind::Term, "terminal:toggle"),
];

/// " (⌘⇧P)": the first shortcut bound to `command`, for tooltips.
fn shortcut(command: &str, cx: &App) -> String {
    cx.global::<Settings>()
        .keymap
        .keys_for(command)
        .first()
        .map(|k| format!(" ({})", display_keys(k, MAC)))
        .unwrap_or_default()
}

impl Workspace {
    /// ◧ (tree view), ⌘ (command palette) and + (new file).
    fn title_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        fn run(
            command: &'static str,
            cx: &mut Context<Workspace>,
        ) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
            cx.listener(move |this, _, window, cx| this.run_command(command, window, cx))
        }
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                ferrum_ui::sidebar_toggle("toggle-tree", self.sidebar_shown())
                    .on_click(run("tree-view:toggle", cx)),
            )
            .child(ferrum_ui::settings_button("open-settings"))
            .child(
                ferrum_ui::command_button(
                    "command-palette",
                    format!("Command palette{}", shortcut("command-palette:toggle", cx)),
                )
                .on_click(run("command-palette:toggle", cx)),
            )
            .child(
                ferrum_ui::title_button(
                    "new-file",
                    IconName::Plus,
                    format!("New file{}", shortcut("application:new-file", cx)),
                )
                .on_click(run("application:new-file", cx)),
            )
    }

    /// The sidebar on the window backdrop: title row, project, file tree.
    pub(super) fn render_sidebar_column(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let tree = self.render_sidebar()?;
        Some(
            v_flex()
                .flex_none()
                .h_full()
                .w(self.tree_width)
                .child(
                    ferrum_ui::title_row("sidebar-title", true, window)
                        .child(div().flex_1())
                        .child(self.title_controls(cx)),
                )
                .child(div().px_1().child(self.project_bar.clone()))
                .child(div().flex_1().min_h_0().child(tree))
                .into_any_element(),
        )
    }

    /// The editor card's header: window controls and project while the
    /// sidebar is hidden, the tabs of a single pane, the tool toggles.
    pub(super) fn render_editor_header(
        &self,
        tabs: Option<Vec<AnyElement>>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let sidebar = self.sidebar_shown();
        let mut toggles = h_flex().flex_none().gap_0p5();
        for (kind, command) in TOP_BAR_TOOLS {
            toggles = toggles.child(self.render_tool_toggle(kind, command, cx));
        }
        ferrum_ui::title_row("editor-header", !sidebar, window)
            .gap_1()
            .border_b_1()
            .border_color(colors.border_variant)
            .when(!sidebar, |row| {
                row.child(self.title_controls(cx)).child(div().w(px(6.)))
            })
            .map(|row| match tabs {
                Some(tabs) => row.child(
                    h_flex()
                        .id("tabs")
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .overflow_x_scroll()
                        .children(tabs),
                ),
                None => row.child(div().flex_1()),
            })
            .when(!sidebar, |row| {
                row.child(
                    div()
                        .flex_none()
                        .max_w(px(320.))
                        .child(self.project_bar.clone()),
                )
                // Sets the project's "open in" buttons apart from the toggles.
                .child(
                    div()
                        .flex_none()
                        .w(px(1.))
                        .h(px(16.))
                        .mx_1()
                        .bg(colors.border_variant),
                )
            })
            .child(toggles)
            .into_any_element()
    }

    fn render_tool_toggle(
        &self,
        kind: ToolKind,
        command: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = self.tool_open(kind);
        let name = kind.title().to_lowercase();
        let verb = if open { "Hide" } else { "Show" };
        let tip = format!("{verb} {} pane{}", kind.title(), shortcut(command, cx));
        let button = div()
            .debug_selector(move || format!("top-bar-{name}"))
            .child(
                ferrum_ui::title_button(
                    SharedString::from(format!("top-{}", kind.title())),
                    tool_icon(kind),
                    tip,
                )
                .toggle_state(open)
                .when(open, |b| b.icon_color(Color::Accent))
                .on_click(
                    cx.listener(move |this, _, window, cx| this.toggle_tool(kind, window, cx)),
                ),
            );
        let Some((label, external)) = external_app(kind) else {
            return button.into_any_element();
        };
        let weak = cx.weak_entity();
        right_click_menu::<ContextMenu>(SharedString::from(format!("top-menu-{}", kind.title())))
            .trigger(move |_, _, _| button)
            .menu(move |window, cx| {
                let weak = weak.clone();
                ContextMenu::build(window, cx, move |menu, _, _| {
                    menu.entry(label, None, move |window, cx| {
                        weak.update(cx, |w, cx| w.run_command(external, window, cx))
                            .ok();
                    })
                })
            })
            .into_any_element()
    }
}
