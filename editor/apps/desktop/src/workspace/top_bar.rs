//! The bar at the top: the shared project bar (name, path, branch,
//! switcher) with the [Git] [Agent] [Term] toggles at its right end. Each
//! opens or closes its tool as a pane inside the window; right-clicking
//! Git or Term offers the stand-alone app instead.

use ferrum_ui::components::{ContextMenu, IconButton, IconSize, right_click_menu};
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

impl Workspace {
    pub(super) fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let mut buttons = h_flex()
            .gap_0p5()
            .p_0p5()
            .rounded_md()
            .bg(colors.element_background);
        for (kind, command) in TOP_BAR_TOOLS {
            buttons = buttons.child(self.render_tool_toggle(kind, command, cx));
        }
        h_flex()
            .flex_none()
            .w_full()
            .child(div().flex_1().min_w_0().child(self.project_bar.clone()))
            .child(
                h_flex()
                    .flex_none()
                    .h(px(30.))
                    .px_2()
                    .bg(colors.title_bar_background)
                    .border_b_1()
                    .border_color(colors.border_variant)
                    .child(buttons),
            )
    }

    fn render_tool_toggle(
        &self,
        kind: ToolKind,
        command: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = self.tool_open(kind);
        let name = kind.title().to_lowercase();
        let keys = cx
            .global::<Settings>()
            .keymap
            .keys_for(command)
            .first()
            .map(|k| format!(" ({})", display_keys(k, MAC)))
            .unwrap_or_default();
        let verb = if open { "Hide" } else { "Show" };
        let tip = format!("{verb} {} pane{keys}", kind.title());
        let button = div()
            .debug_selector(move || format!("top-bar-{name}"))
            .child(
                IconButton::new(
                    SharedString::from(format!("top-{}", kind.title())),
                    tool_icon(kind),
                )
                .icon_size(IconSize::Small)
                .toggle_state(open)
                .when(open, |b| b.icon_color(Color::Accent))
                .tooltip(Tooltip::text(tip))
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
