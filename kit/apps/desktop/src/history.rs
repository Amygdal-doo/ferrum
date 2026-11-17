//! The history panel at the right of the tool card: "Keep history" toggle,
//! save the current input under a name, saved inputs, recent inputs. A
//! click on an entry puts it back into the tool.

use chrono::{DateTime, Utc};
use ferrum_ui::components::Checkbox;
use ferrum_ui::prelude::*;
use ferrumkit_core::Tool;
use ferrumkit_core::store::HISTORY_LIMIT;
use ferrumkit_core::timestamp::relative;

use crate::app::FerrumKit;
use crate::widgets::{caption, hint};

const WIDTH: f32 = 270.;
const PREVIEW_CHARS: usize = 80;

/// One clickable entry with a delete button.
fn entry_row(
    id: SharedString,
    title: String,
    detail: Option<String>,
    cx: &mut Context<FerrumKit>,
    on_open: impl Fn(&mut FerrumKit, &mut Context<FerrumKit>) + 'static,
    on_delete: impl Fn(&mut FerrumKit, &mut Context<FerrumKit>) + 'static,
) -> AnyElement {
    let colors = cx.theme().colors();
    h_flex()
        .id(id.clone())
        .gap_1()
        .px_2()
        .py_1()
        .rounded_sm()
        .cursor_pointer()
        .hover(|s| s.bg(colors.element_hover))
        .on_click(cx.listener(move |this, _, _, cx| on_open(this, cx)))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().text_sm().font_buffer(cx).truncate().child(title))
                .children(
                    detail.map(|d| Label::new(d).size(LabelSize::XSmall).color(Color::Muted)),
                ),
        )
        .child(
            IconButton::new(SharedString::from(format!("{id}-delete")), IconName::Close)
                .icon_size(IconSize::XSmall)
                .tooltip(Tooltip::text("Remove"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    on_delete(this, cx);
                })),
        )
        .into_any_element()
}

pub fn render(app: &FerrumKit, tool: Tool, cx: &mut Context<FerrumKit>) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let keep = app.store.keeps_history(tool);
    let now = Utc::now();

    let saved: Vec<AnyElement> = app
        .store
        .saved(tool)
        .to_vec()
        .into_iter()
        .map(|saved| {
            let name = saved.name.clone();
            let inputs = saved.inputs.clone();
            entry_row(
                SharedString::from(format!("saved-{name}")),
                saved.name.clone(),
                Some(ferrumkit_core::store::preview(&saved.inputs, PREVIEW_CHARS)),
                cx,
                move |this, cx| this.restore(inputs.clone(), cx),
                move |this, cx| this.update_store(cx, |s| _ = s.remove_saved(tool, &name)),
            )
        })
        .collect();

    let recent: Vec<AnyElement> = app
        .store
        .history(tool)
        .to_vec()
        .into_iter()
        .enumerate()
        .map(|(index, entry)| {
            let when = DateTime::<Utc>::from_timestamp(entry.at, 0)
                .map(|at| relative(at, now))
                .unwrap_or_default();
            let inputs = entry.inputs.clone();
            entry_row(
                SharedString::from(format!("history-{index}")),
                entry.preview(PREVIEW_CHARS),
                Some(when),
                cx,
                move |this, cx| this.restore(inputs.clone(), cx),
                move |this, cx| this.update_store(cx, |s| _ = s.remove_history(tool, index)),
            )
        })
        .collect();

    let keep_toggle = Checkbox::new("keep-history", keep.into())
        .label("Keep history")
        .on_click(cx.listener(move |this, state: &ToggleState, _, cx| {
            let keep = *state == ToggleState::Selected;
            this.update_store(cx, |s| s.set_keeps_history(tool, keep));
        }));
    let keep_hint = if tool.sensitive() && !keep {
        "Off by default for this tool: its input is usually a secret."
    } else if keep {
        "The last inputs are kept in ~/.ferrum/kit/. Turning this off deletes them."
    } else {
        "Inputs of this tool are not recorded."
    };
    let has_history = !recent.is_empty();

    v_flex()
        .id("history-panel")
        .w(px(WIDTH))
        .flex_none()
        .h_full()
        .border_l_1()
        .border_color(colors.border_variant)
        .p_3()
        .gap_3()
        .overflow_y_scroll()
        .child(v_flex().gap_1().child(keep_toggle).child(hint(keep_hint)))
        .child(
            v_flex().gap_1().child(caption("Save input as")).child(
                h_flex()
                    .gap_1()
                    .child(div().flex_1().min_w_0().child(app.save_name.clone()))
                    .child(
                        Button::new("save-input", "Save")
                            .style(ButtonStyle::Outlined)
                            .size(ButtonSize::Compact)
                            .on_click(cx.listener(|this, _, _, cx| this.save_input(cx))),
                    ),
            ),
        )
        .child(
            v_flex()
                .gap_0p5()
                .child(caption("Saved"))
                .when(saved.is_empty(), |d| d.child(hint("Nothing saved yet.")))
                .children(saved),
        )
        .child(
            v_flex()
                .gap_0p5()
                .child(
                    h_flex()
                        .justify_between()
                        .child(caption(format!("Recent (last {HISTORY_LIMIT})")))
                        .when(has_history, |d| {
                            d.child(
                                Button::new("clear-history", "Clear")
                                    .style(ButtonStyle::Subtle)
                                    .size(ButtonSize::Compact)
                                    .label_size(LabelSize::Small)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.update_store(cx, |s| s.clear_history(tool))
                                    })),
                            )
                        }),
                )
                .when(!has_history && keep, |d| {
                    d.child(hint("Inputs show up here after a pause in typing."))
                })
                .children(recent),
        )
        .into_any_element()
}
