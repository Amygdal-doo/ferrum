//! Window chrome: the ◧ ⌘ + controls of the title row and the compact
//! tools (environment, webhooks, cookies, import) at the end of the
//! request card's header.

use crate::app::FerrumApi;
use crate::components::select;
use crate::state::Dialog;
use ferrum_ui::components::{Clickable as _, DropdownStyle, IconName, h_flex};
use gpui::{AnyElement, Context, SharedString, Window, prelude::*};

impl FerrumApi {
    /// ◧ (sidebar), ⌘ (project switcher) and + (new request): in the
    /// sidebar's title row, or first in the request card's header while the
    /// sidebar is hidden.
    pub fn render_title_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                ferrum_ui::sidebar_toggle("toggle-sidebar", self.state.sidebar_open)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
            )
            .child(
                ferrum_ui::command_button("switch-project", "Switch project (⌥⌘O)").on_click(
                    cx.listener(|this, _, window, cx| {
                        this.project_bar
                            .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
                    }),
                ),
            )
            .child(
                ferrum_ui::title_button("title-new-request", IconName::Plus, "New request")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.new_tab();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    /// Environment picker and the Webhooks, Cookies and Import buttons.
    pub fn render_header_tools(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.state.active_env;
        let current: SharedString = self
            .state
            .active_environment()
            .map_or_else(|| "No environment".into(), |e| e.name.clone().into());
        let mut options = vec![(SharedString::from("No environment"), None)];
        options.extend(
            self.state
                .environments
                .iter()
                .map(|e| (SharedString::from(e.name.clone()), Some(e.id))),
        );
        let env_select = select(
            "active-environment",
            current,
            options,
            |this, id, _| this.set_active_environment(id),
            window,
            cx,
        )
        .style(DropdownStyle::Ghost);

        h_flex()
            .flex_none()
            .gap_0p5()
            .child(env_select)
            .when_some(active, |this, id| {
                this.child(
                    ferrum_ui::title_button(
                        "edit-environment",
                        IconName::Pencil,
                        "Edit the active environment",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_environment(id);
                        cx.notify();
                    })),
                )
            })
            .child(
                ferrum_ui::title_button(
                    "webhooks",
                    IconName::Zap,
                    "Webhooks: capture incoming requests on a local port",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.state.dialog = Some(Dialog::Webhooks);
                    cx.notify();
                })),
            )
            .child(
                ferrum_ui::title_button("cookies", IconName::Archive, "Cookies: view and clear")
                    .on_click(cx.listener(|this, _, _, cx| this.open_cookies(cx))),
            )
            .child(
                ferrum_ui::title_button(
                    "import",
                    IconName::Download,
                    "Import a Postman collection or a cURL command",
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.state.dialog = Some(Dialog::Import {
                        curl: String::new(),
                        error: None,
                    });
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.state.sidebar_open = !self.state.sidebar_open;
        cx.notify();
    }
}
