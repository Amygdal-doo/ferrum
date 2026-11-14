//! Stash list (Stashes page).

use crate::app::GitView;
use crate::components::actions::Action;
use crate::components::icons::Icon;
use crate::components::now_secs;
use crate::state::Selection;
use ferrum_ui::components::{Color, IconSize, Label, LabelCommon, LabelSize, h_flex, v_flex};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::RepoId;
use ferrumgit_domain::text::relative_time;
use gpui::{AnyElement, Context, MouseButton, MouseDownEvent, SharedString, div, prelude::*, px};

pub fn stash_list(app: &GitView, id: RepoId, cx: &mut Context<GitView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let (selected_fill, selected_text, selected_muted) = crate::components::selected_colors(&theme);
    let view = app.state.view(id);
    let stashes = view
        .data
        .as_ref()
        .map(|d| d.snapshot.stashes.clone())
        .unwrap_or_default();
    let now = now_secs();
    let rows: Vec<AnyElement> = stashes
        .into_iter()
        .map(|s| {
            let i = s.index;
            let selected = view.selection == Selection::Stash(i);
            let when = relative_time(s.time, now);
            let (text, muted) = if selected {
                (Color::Custom(selected_text), Color::Custom(selected_muted))
            } else {
                (Color::Default, Color::Muted)
            };
            h_flex()
                .id(SharedString::from(format!("stash-{}", s.oid)))
                .gap_3()
                .px_3()
                .py_2()
                .rounded_lg()
                .when(selected, |d| d.bg(selected_fill))
                .when(!selected, |d| d.hover(|st| st.bg(colors.element_hover)))
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.navigate(id, Selection::Stash(i), cx);
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |app, event: &MouseDownEvent, _, cx| {
                        app.open_menu(
                            event.position,
                            vec![
                                Action::StashApply(i),
                                Action::StashPop(i),
                                Action::StashDrop(i),
                            ],
                            cx,
                        );
                    }),
                )
                .child(Icon::Layers.icon().size(IconSize::Small).color(text))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(Label::new(s.message.clone()).color(text).truncate())
                        .child(
                            Label::new(format!("{} · {when}", s.refname()))
                                .size(LabelSize::XSmall)
                                .color(muted),
                        ),
                )
                .into_any_element()
        })
        .collect();
    let empty = rows.is_empty();
    v_flex()
        .size_full()
        .child(
            h_flex()
                .flex_none()
                .h(px(36.))
                .px_3()
                .border_b_1()
                .border_color(colors.border)
                .child(
                    Label::new("Stashes")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        )
        .child(
            v_flex()
                .id("stash-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_2()
                .when(empty, |d| {
                    d.child(
                        div().p_6().flex().justify_center().child(
                            Label::new(
                                "No stashes. Use Save stash in the header to stash your changes.",
                            )
                            .color(Color::Muted),
                        ),
                    )
                })
                .children(rows),
        )
        .into_any_element()
}
