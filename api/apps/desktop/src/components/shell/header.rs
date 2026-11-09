use crate::app::FerrumApi;
use crate::components::select;
use crate::state::Dialog;
use ferrum_ui::toolbar;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};
use ui::{
    Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _, Color, Divider, Label,
    LabelCommon as _, LabelSize, Tooltip,
};

impl FerrumApi {
    pub fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
        );

        toolbar(cx)
            .child(Label::new("FerrumAPI").weight(gpui::FontWeight::SEMIBOLD))
            .child(Divider::vertical())
            .child(
                Label::new("Environment")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(env_select)
            .when_some(active, |this, id| {
                this.child(
                    Button::new("edit-environment", "Edit")
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .tooltip(Tooltip::text("Edit the active environment"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_environment(id);
                            cx.notify();
                        })),
                )
            })
            .child(div().flex_1())
            .child(
                Button::new("cookies", "Cookies")
                    .style(ButtonStyle::Subtle)
                    .tooltip(Tooltip::text("View and clear stored cookies"))
                    .on_click(cx.listener(|this, _, _, cx| this.open_cookies(cx))),
            )
            .child(
                Button::new("import", "Import")
                    .style(ButtonStyle::Outlined)
                    .tooltip(Tooltip::text(
                        "Import a Postman collection or a cURL command",
                    ))
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
}
