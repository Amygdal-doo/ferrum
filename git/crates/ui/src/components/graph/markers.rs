//! A host's markers above the commits: working-tree states that are not
//! commits (FerrumEngineer's agent checkpoints), newest first, each with a
//! "Restore to here…" button that asks the host
//! ([`crate::RestoreMarker`]); the host confirms and restores.

use crate::app::GitView;
use crate::components::widgets::small_button;
use crate::embed::RestoreMarker;
use ferrum_ui::components::{
    ButtonCommon, Clickable, Color, Label, LabelCommon, LabelSize, Tooltip, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

/// Markers shown at most before "N more".
const SHOWN: usize = 8;

pub fn markers_section(app: &GitView, cx: &mut Context<GitView>) -> Option<AnyElement> {
    let (title, markers) = app.markers.as_ref()?;
    if markers.is_empty() {
        return None;
    }
    let colors = cx.theme().colors().clone();
    let accent = colors.text_accent;
    let mut list = v_flex().gap_0p5();
    for marker in markers.iter().take(SHOWN) {
        let id = marker.id;
        list = list.child(
            h_flex()
                .id(SharedString::from(format!("history-marker-{id}")))
                .gap_2()
                .pl_3()
                .pr_2()
                .py_1()
                .child(
                    div()
                        .flex_none()
                        .size(px(9.))
                        .rounded_full()
                        .border_2()
                        .border_color(accent),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            Label::new(marker.title.clone())
                                .size(LabelSize::Small)
                                .single_line()
                                .truncate(),
                        )
                        .child(
                            Label::new(marker.detail.clone())
                                .size(LabelSize::XSmall)
                                .color(Color::Muted)
                                .single_line()
                                .truncate(),
                        ),
                )
                .child(
                    small_button(
                        SharedString::from(format!("history-marker-restore-{id}")),
                        "Restore to here…",
                    )
                    .tooltip(Tooltip::text(
                        "Put the files back as they were before this point",
                    ))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(RestoreMarker { id });
                    })),
                ),
        );
    }
    let more = markers.len().saturating_sub(SHOWN);
    Some(
        v_flex()
            .flex_none()
            .py_1()
            .border_b_1()
            .border_color(colors.border)
            .child(
                h_flex().px_3().pb_0p5().child(
                    Label::new(title.clone())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                ),
            )
            .child(list)
            .when(more > 0, |this| {
                this.child(
                    div().px_3().child(
                        Label::new(format!("{more} older"))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
                )
            })
            .into_any_element(),
    )
}
