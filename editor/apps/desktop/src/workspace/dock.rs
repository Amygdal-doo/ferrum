//! The right dock: Agents | Debug tabs over the agent sessions list or
//! the debug panel. ⌥⌘A and ⇧⌘Y show / hide it on their tab.

use ferrum_ui::components::{IconButton, IconName, IconSize};
use ferrum_ui::prelude::*;

use super::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightTab {
    Agents,
    Debug,
}

impl Workspace {
    /// Shows the dock on `tab`, or hides it when it already shows `tab`.
    pub(super) fn toggle_right(
        &mut self,
        tab: RightTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.right_panel == Some(tab) {
            self.right_panel = None;
            self.focus_active(window, cx);
        } else {
            self.right_panel = Some(tab);
        }
        cx.notify();
    }

    pub(super) fn show_right(&mut self, tab: RightTab, cx: &mut Context<Self>) {
        self.right_panel = Some(tab);
        cx.notify();
    }

    pub(super) fn render_right_dock(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tab = self.right_panel?;
        let colors = cx.theme().colors().clone();
        let content: AnyElement = match tab {
            RightTab::Agents => self.render_agents_panel(cx),
            RightTab::Debug => self.debug_panel.clone().into_any_element(),
        };
        let button = |id: &'static str, icon: IconName, active: bool, tip: &'static str| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .toggle_state(active)
                .when(active, |b| b.icon_color(Color::Accent))
                .tooltip(Tooltip::text(tip))
        };
        Some(
            v_flex()
                .size_full()
                .bg(colors.panel_background)
                .child(
                    h_flex()
                        .flex_none()
                        .h(px(28.))
                        .px_1()
                        .gap_1()
                        .border_b_1()
                        .border_color(colors.border)
                        .child(
                            button(
                                "dock-agents",
                                IconName::Sparkle,
                                tab == RightTab::Agents,
                                "Agents (⌥⌘A)",
                            )
                            .on_click(
                                cx.listener(|this, _, _, cx| this.show_right(RightTab::Agents, cx)),
                            ),
                        )
                        .child(
                            button(
                                "dock-debug",
                                IconName::Debug,
                                tab == RightTab::Debug,
                                "Debug (⇧⌘Y)",
                            )
                            .on_click(
                                cx.listener(|this, _, _, cx| this.show_right(RightTab::Debug, cx)),
                            ),
                        )
                        .child(div().flex_1())
                        .child(
                            IconButton::new("dock-close", IconName::Close)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text("Hide"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.right_panel = None;
                                    this.focus_active(window, cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(div().flex_1().min_h_0().child(content))
                .into_any_element(),
        )
    }
}
