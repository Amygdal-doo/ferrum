//! Request tab strip: switch, close, new, dirty dot, double-click rename.

use crate::app::FerrumApi;
use crate::components::fields::{FieldEvent, FieldSpec};
use crate::components::request_badge;
use crate::state::tabs::display_title;
use gpui::{AnyElement, ClickEvent, Context, SharedString, Window, div, prelude::*, px};
use ui::{
    ActiveTheme as _, ButtonCommon as _, Clickable as _, IconButton, IconName, IconSize, Tooltip,
    h_flex,
};

impl FerrumApi {
    pub(super) fn render_tab_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let active = self.state.active_tab;
        let list = self.state.tabs.clone();
        let mut tabs = Vec::with_capacity(list.len());
        for tab in list {
            let id = tab.id;
            let selected = active == Some(id);
            let title = display_title(&tab);
            let editing = self.state.renaming_tab == Some(id);
            let dirty_title = if tab.origin.is_some() {
                "Unsaved changes"
            } else {
                "Not saved to a collection"
            };
            let label = if editing {
                let key = format!("tabname:{id}");
                let input = self.field(
                    FieldSpec::new(key.clone())
                        .initial(title.clone())
                        .autofocus(),
                    move |this, event, _, cx| {
                        if matches!(event, FieldEvent::Submit | FieldEvent::Blurred)
                            && this.state.renaming_tab == Some(id)
                        {
                            this.state.renaming_tab = None;
                            let name = this.fields.text(&key, cx).unwrap_or_default();
                            this.state
                                .update_tab(id, |t| t.title = name.trim().to_owned());
                        }
                    },
                    window,
                    cx,
                );
                div().w(px(144.)).child(input).into_any_element()
            } else {
                div().truncate().child(title.clone()).into_any_element()
            };
            tabs.push(
                h_flex()
                    .id(SharedString::from(format!("tab-{id}")))
                    .flex_none()
                    .max_w(px(224.))
                    .h_8()
                    .gap_1p5()
                    .px_3()
                    .rounded_t_md()
                    .text_xs()
                    .map(|this| {
                        if selected {
                            this.bg(colors.tab_active_background)
                                .border_x_1()
                                .border_t_1()
                                .border_color(colors.border)
                                .text_color(colors.text)
                        } else {
                            this.text_color(colors.text_muted)
                                .hover(|s| s.text_color(colors.text))
                        }
                    })
                    .tooltip(Tooltip::text(title))
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        if event.click_count() >= 2 {
                            this.state.renaming_tab = Some(id);
                        } else if this.state.renaming_tab != Some(id) {
                            this.activate(id);
                        }
                        cx.notify();
                    }))
                    .child(request_badge(tab.request.kind, tab.request.method, cx))
                    .child(label)
                    .when(tab.dirty, |this| {
                        this.child(
                            div()
                                .id(SharedString::from(format!("tab-dirty-{id}")))
                                .size_1p5()
                                .flex_none()
                                .rounded_full()
                                .bg(colors.text_muted)
                                .tooltip(Tooltip::text(dirty_title)),
                        )
                    })
                    .child(
                        IconButton::new(
                            SharedString::from(format!("tab-close-{id}")),
                            IconName::Close,
                        )
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Close tab"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_tab(id);
                            cx.notify();
                        })),
                    )
                    .into_any_element(),
            );
        }

        h_flex()
            .id("request-tabs")
            .flex_none()
            .h_9()
            .items_end()
            .gap_0p5()
            .px_2()
            .overflow_x_scroll()
            .bg(colors.tab_bar_background)
            .border_b_1()
            .border_color(colors.border)
            .children(tabs)
            .child(
                h_flex()
                    .mb_1()
                    .child(
                        IconButton::new("new-tab", IconName::Plus)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("New HTTP request tab"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.new_tab();
                                cx.notify();
                            })),
                    )
                    .child(crate::components::dialogs::new_request_menu(
                        "new-tab-kind",
                        cx,
                    )),
            )
            .into_any_element()
    }
}
