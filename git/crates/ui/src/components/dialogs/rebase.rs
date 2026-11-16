//! Interactive rebase: reorder (drag or ⌥↑/⌥↓) and pick an action per
//! commit (buttons or git's letters), preview the result, then run git's
//! own `rebase -i` with the generated todo list.

use super::{DialogState, WIDE, modal, width};
use crate::app::GitView;
use crate::components::widgets::{ghost_button, hint, primary_button, small_button};
use ferrum_ui::TextInput;
use ferrum_ui::components::{
    Clickable, Color, Disableable, Label, LabelCommon, LabelSize, Tooltip, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{Oid, RebaseAction, RebasePlan, RebaseStep, RepoOp};
use gpui::{
    AnyElement, Context, Entity, FocusHandle, KeyDownEvent, SharedString, Window, div, prelude::*,
    px,
};

pub struct RebaseForm {
    pub plan: RebasePlan,
    /// The order before any change (to tell a no-op).
    pub original: Vec<Oid>,
    pub selected: usize,
    /// New message of the selected step while it is reworded.
    pub message: Entity<TextInput>,
    pub preview: bool,
    /// Keyboard focus of the commit list.
    pub focus: FocusHandle,
}

/// A step being dragged to a new position.
#[derive(Clone)]
pub struct DraggedStep {
    index: usize,
    label: SharedString,
}

impl Render for DraggedStep {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(colors.elevated_surface_background)
            .border_1()
            .border_color(colors.border)
            .shadow_md()
            .text_size(px(12.))
            .child(self.label.clone())
    }
}

impl RebaseForm {
    pub fn new(
        base: Oid,
        steps: Vec<RebaseStep>,
        window: &mut Window,
        cx: &mut Context<GitView>,
    ) -> Self {
        let original = steps.iter().map(|s| s.oid.clone()).collect();
        let message = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("New commit message")
                .multiline(4)
        });
        Self {
            plan: RebasePlan::new(base, steps),
            original,
            selected: 0,
            message,
            preview: false,
            focus: cx.focus_handle(),
        }
    }

    /// Copies the message being edited into its step.
    fn store_message(&mut self, cx: &Context<GitView>) {
        let text = self.message.read(cx).text().to_owned();
        if let Some(step) = self.plan.steps.get_mut(self.selected)
            && step.action == RebaseAction::Reword
        {
            step.reword = Some(text);
        }
    }

    /// Shows the selected step's message in the editor.
    fn load_message(&self, cx: &mut Context<GitView>) {
        let Some(step) = self.plan.steps.get(self.selected) else {
            return;
        };
        let text = step.reword.clone().unwrap_or_else(|| step.message.clone());
        self.message
            .update(cx, |input, cx| input.set_text(text, cx));
    }

    fn select(&mut self, index: usize, cx: &mut Context<GitView>) {
        if self.plan.steps.is_empty() {
            return;
        }
        self.store_message(cx);
        self.selected = index.min(self.plan.steps.len() - 1);
        self.load_message(cx);
    }

    fn set_action(&mut self, index: usize, action: RebaseAction, cx: &mut Context<GitView>) {
        self.select(index, cx);
        if let Some(step) = self.plan.steps.get_mut(index) {
            step.action = action;
            if action == RebaseAction::Reword && step.reword.is_none() {
                step.reword = Some(step.message.clone());
            }
        }
        self.load_message(cx);
    }

    fn move_to(&mut self, from: usize, to: usize, cx: &mut Context<GitView>) {
        self.store_message(cx);
        self.plan.move_step(from, to);
        self.selected = to.min(self.plan.steps.len().saturating_sub(1));
        self.load_message(cx);
    }

    /// The plan with the message being edited, ready to run.
    pub fn finished_plan(&mut self, cx: &Context<GitView>) -> RebasePlan {
        self.store_message(cx);
        self.plan.clone()
    }
}

fn form(app: &mut GitView) -> Option<&mut RebaseForm> {
    match &mut app.dialog {
        Some(DialogState::Rebase(f)) => Some(f),
        _ => None,
    }
}

impl GitView {
    /// Loads `base..HEAD` and opens the interactive rebase dialog.
    pub fn open_interactive_rebase(&mut self, base: Oid, window: &Window, cx: &mut Context<Self>) {
        let Some(entry) = self.state.active_entry() else {
            return;
        };
        let services = self.services.clone();
        cx.spawn_in(window, async move |this, cx| {
            let b = base.clone();
            let result = crate::commands::run(&services.clone(), async move {
                services.rebase_steps(&entry.path, &b).await
            })
            .await;
            let opened = this.update_in(cx, |app, window, cx| match result {
                Ok(steps) => {
                    app.menu = None;
                    app.dialog_subscriptions.clear();
                    let rebase = RebaseForm::new(base, steps, window, cx);
                    let sub = cx.subscribe_in(&rebase.message, window, |app, _, _, _, cx| {
                        if let Some(f) = form(app) {
                            f.store_message(cx);
                        }
                        cx.notify();
                    });
                    app.dialog_subscriptions.push(sub);
                    window.focus(&rebase.focus, cx);
                    app.dialog = Some(DialogState::Rebase(rebase));
                    cx.notify();
                }
                Err(e) => app.notify(e, true, cx),
            });
            if let Err(error) = opened {
                tracing::debug!(%error, "view closed before the rebase plan loaded");
            }
        })
        .detach();
    }

    fn rebase_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(f) = form(self) else { return };
        // Typing in the message editor is not a shortcut.
        if !f.focus.is_focused(window) || f.preview {
            return;
        }
        let keys = &event.keystroke;
        let m = keys.modifiers;
        if m.platform || m.control {
            return;
        }
        let i = f.selected;
        let last = f.plan.steps.len().saturating_sub(1);
        match keys.key.as_str() {
            "up" if m.alt => f.move_to(i, i.saturating_sub(1), cx),
            "down" if m.alt => f.move_to(i, (i + 1).min(last), cx),
            "up" => f.select(i.saturating_sub(1), cx),
            "down" => f.select(i + 1, cx),
            key => match RebaseAction::from_key(key) {
                Some(action) if !m.alt && !m.shift => f.set_action(i, action, cx),
                _ => return,
            },
        }
        cx.stop_propagation();
        cx.notify();
    }
}

fn step_row(f: &RebaseForm, i: usize, step: &RebaseStep, cx: &mut Context<GitView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let selected = f.selected == i;
    let action = step.action;
    let dropped = action == RebaseAction::Drop;
    let summary = step
        .final_message()
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    let chips: Vec<AnyElement> = RebaseAction::ALL
        .into_iter()
        .map(|a| {
            let on = a == action;
            div()
                .id(SharedString::from(format!("rb-{i}-{}", a.key())))
                .w(px(20.))
                .h(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .cursor_pointer()
                .text_size(px(11.))
                .when(on, |d| {
                    d.bg(colors.text_accent).text_color(colors.background)
                })
                .when(!on, |d| {
                    d.text_color(colors.text_muted)
                        .hover(|s| s.bg(colors.element_hover))
                })
                .tooltip(Tooltip::text(format!("{} ({})", a.label(), a.key())))
                .child(a.key().to_uppercase())
                .on_click(cx.listener(move |app, _, window, cx| {
                    if let Some(f) = form(app) {
                        f.set_action(i, a, cx);
                        window.focus(&f.focus, cx);
                    }
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();
    let drag = DraggedStep {
        index: i,
        label: format!("{} {summary}", step.oid.short()).into(),
    };
    let row = h_flex()
        .id(SharedString::from(format!("rb-step-{}", step.oid)))
        .gap_2()
        .px_2()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |d| d.bg(colors.element_selected))
        .when(!selected, |d| d.hover(|s| s.bg(colors.element_hover)))
        .drag_over::<DraggedStep>(|s, _, _, cx| s.bg(cx.theme().colors().drop_target_background))
        .on_drag(drag, |d, _, _, cx| cx.new(|_| d.clone()))
        .on_drop(cx.listener(move |app, d: &DraggedStep, window, cx| {
            if let Some(f) = form(app) {
                f.move_to(d.index, i, cx);
                window.focus(&f.focus, cx);
            }
            cx.notify();
        }))
        .on_click(cx.listener(move |app, _, window, cx| {
            if let Some(f) = form(app) {
                f.select(i, cx);
                window.focus(&f.focus, cx);
            }
            cx.notify();
        }))
        .child(Label::new("⋮⋮").size(LabelSize::Small).color(Color::Muted))
        .child(h_flex().gap_0p5().children(chips))
        .child(
            Label::new(step.oid.short().to_owned())
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
        .when(action.melds(), |d| {
            d.child(Label::new("↳").size(LabelSize::Small).color(Color::Accent))
        })
        .child(
            div().flex_1().min_w_0().child(
                Label::new(summary)
                    .size(LabelSize::Small)
                    .when(dropped, LabelCommon::strikethrough)
                    .color(if dropped {
                        Color::Muted
                    } else {
                        Color::Default
                    })
                    .truncate(),
            ),
        );
    let editor = (selected && action == RebaseAction::Reword)
        .then(|| div().pl(px(28.)).pb_1().child(f.message.clone()));
    v_flex().child(row).children(editor).into_any_element()
}

pub fn render(app: &mut GitView, cx: &mut Context<GitView>) -> AnyElement {
    let Some(DialogState::Rebase(f)) = &app.dialog else {
        return div().into_any_element();
    };
    let base = f.plan.base.short().to_owned();
    let invalid = f.plan.validate().err();
    let noop = f.plan.is_noop(&f.original);
    let preview = f.preview;
    let body: AnyElement = if preview {
        let rows: Vec<AnyElement> = f
            .plan
            .preview()
            .into_iter()
            .map(|c| {
                let mut note = Vec::new();
                if c.sources.len() > 1 {
                    note.push(format!("{} commits melded", c.sources.len()));
                }
                if c.reworded {
                    note.push("reworded".into());
                }
                if c.stops {
                    note.push("stops to edit".into());
                }
                h_flex()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .child(
                        Label::new(c.sources.join(" + "))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Label::new(c.summary).size(LabelSize::Small).truncate()),
                    )
                    .children((!note.is_empty()).then(|| {
                        Label::new(note.join(" · "))
                            .size(LabelSize::XSmall)
                            .color(Color::Accent)
                    }))
                    .into_any_element()
            })
            .collect();
        let empty = rows.is_empty();
        v_flex()
            .gap_1()
            .child(hint(format!(
                "Result on top of {base}, oldest first. Nothing has run yet."
            )))
            .when(empty, |d| {
                d.child(hint(
                    "Every commit is dropped: the branch moves to the base.",
                ))
            })
            .child(
                v_flex()
                    .id("rebase-preview")
                    .max_h(px(420.))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    } else {
        let rows: Vec<AnyElement> = f
            .plan
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| step_row(f, i, s, cx))
            .collect();
        v_flex()
            .gap_1()
            .child(hint(
                "Oldest first. Drag or ⌥↑ ⌥↓ to reorder, ↑ ↓ to select; \
                 P R E S F D: pick, reword, edit, squash, fixup, drop.",
            ))
            .child(
                v_flex()
                    .id("rebase-steps")
                    .track_focus(&f.focus)
                    .on_key_down(cx.listener(GitView::rebase_key))
                    .max_h(px(420.))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    };
    let error = invalid
        .clone()
        .map(|e| Label::new(e).size(LabelSize::Small).color(Color::Error));
    let blocked = invalid.is_some() || noop;
    modal(format!("Interactive rebase onto {base}"), cx)
        .width(width(WIDE))
        .child(body)
        .children(error)
        .when(noop && !preview, |m| {
            m.child(hint("Change the order or an action to rebase."))
        })
        .footer(
            ghost_button("dialog-cancel", "Cancel")
                .on_click(cx.listener(|app, _, window, cx| app.close_dialog(window, cx))),
        )
        .footer(
            small_button(
                "rebase-preview-toggle",
                if preview { "Edit" } else { "Preview" },
            )
            .disabled(invalid.is_some())
            .on_click(cx.listener(|app, _, window, cx| {
                if let Some(f) = form(app) {
                    f.store_message(cx);
                    f.preview = !f.preview;
                    if !f.preview {
                        window.focus(&f.focus, cx);
                    }
                }
                cx.notify();
            })),
        )
        .footer(
            primary_button("dialog-confirm", "Start rebase")
                .disabled(blocked)
                .on_click(cx.listener(|app, _, window, cx| app.submit_dialog(window, cx))),
        )
        .into_any_element()
}

/// The op for the dialog's primary button, if the plan can run.
pub fn submit(f: &mut RebaseForm, cx: &Context<GitView>) -> Option<RepoOp> {
    let plan = f.finished_plan(cx);
    (plan.validate().is_ok() && !plan.is_noop(&f.original))
        .then_some(RepoOp::InteractiveRebase(plan))
}
