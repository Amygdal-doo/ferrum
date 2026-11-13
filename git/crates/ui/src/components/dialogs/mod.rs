//! In-app modal dialogs. `Dialog` describes what to open; `DialogState`
//! holds the live form (text inputs are GPUI entities).

mod clone;
mod pickers;
mod profiles;
mod pulls;
mod repo_settings;

use crate::app::GitView;
use crate::components::widgets::{confirm_button, field, ghost_button, hint, radio_row};
use crate::state::{Dialog, PromptKind};
use ferrum_ui::components::{
    Checkbox, Clickable, Disableable, Label, LabelSize, ToggleState, v_flex,
};
use ferrum_ui::{Modal, TextInput, TextInputEvent};
use ferrumgit_domain::{RepoId, RepoOp, ResetMode};
use gpui::{AnyElement, Context, Entity, Window, prelude::*, px};

pub enum DialogState {
    Confirm {
        title: String,
        body: String,
        confirm: String,
        destructive: bool,
        op: RepoOp,
    },
    Prompt {
        kind: PromptKind,
        text: Entity<TextInput>,
        message: Option<Entity<TextInput>>,
        checked: bool,
    },
    Reset {
        target: String,
        label: String,
        mode: ResetMode,
    },
    AddRemote {
        name: Entity<TextInput>,
        url: Entity<TextInput>,
    },
    Clone(clone::CloneForm),
    QuickOpen(pickers::Picker),
    PickBranch {
        rebase: bool,
        picker: pickers::Picker,
    },
    RepoSettings(RepoId),
    Profiles(profiles::ProfilesForm),
    ForgeSettings(pulls::ForgeSettingsForm),
    CreatePr(pulls::CreatePrForm),
    PrReview(pulls::ReviewForm),
    PrMerge(pulls::MergeForm),
}

fn short(rev: &str) -> &str {
    if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
        &rev[..7]
    } else {
        rev
    }
}

impl GitView {
    pub fn input(
        &self,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let placeholder = placeholder.to_owned();
        cx.new(|cx| TextInput::new(window, cx).placeholder(placeholder))
    }

    /// Re-renders on edits and submits the dialog on Enter.
    pub fn watch_input(
        &mut self,
        input: &Entity<TextInput>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let subscription =
            cx.subscribe_in(input, window, |app, _, event, window, cx| match event {
                TextInputEvent::Changed => cx.notify(),
                TextInputEvent::Submit => app.submit_dialog(window, cx),
                TextInputEvent::Blurred => {}
            });
        self.dialog_subscriptions.push(subscription);
    }

    pub fn open_dialog(&mut self, dialog: Dialog, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.dialog_subscriptions.clear();
        let mut focus = None;
        let state = match dialog {
            Dialog::Confirm {
                title,
                body,
                confirm,
                destructive,
                op,
            } => DialogState::Confirm {
                title,
                body,
                confirm,
                destructive,
                op,
            },
            Dialog::Prompt { kind, initial } => {
                let (_, field_label, message_label, check) = kind.labels();
                let text = cx.new(|cx| {
                    TextInput::new(window, cx)
                        .placeholder(field_label)
                        .with_text(initial)
                });
                self.watch_input(&text, window, cx);
                let message =
                    message_label.map(|_| cx.new(|cx| TextInput::new(window, cx).multiline(3)));
                if let Some(message) = &message {
                    self.watch_input(message, window, cx);
                }
                focus = Some(text.clone());
                DialogState::Prompt {
                    kind,
                    text,
                    message,
                    checked: check.is_some_and(|c| c.1),
                }
            }
            Dialog::Reset { target, label } => DialogState::Reset {
                target,
                label,
                mode: ResetMode::Mixed,
            },
            Dialog::AddRemote => {
                let has_origin = self
                    .state
                    .active_entry()
                    .and_then(|e| self.state.view(e.id).data)
                    .is_some_and(|d| d.snapshot.remotes.iter().any(|r| r.name == "origin"));
                let initial = if has_origin { "upstream" } else { "origin" };
                let name = cx.new(|cx| TextInput::new(window, cx).with_text(initial));
                let url = self.input("git@github.com:owner/repo.git", window, cx);
                self.watch_input(&name, window, cx);
                self.watch_input(&url, window, cx);
                focus = Some(url.clone());
                DialogState::AddRemote { name, url }
            }
            Dialog::Clone => {
                let form = clone::CloneForm::new(self, window, cx);
                focus = Some(form.url.clone());
                DialogState::Clone(form)
            }
            Dialog::QuickOpen => {
                let picker = pickers::Picker::new(self, "Repository name or path", window, cx);
                focus = Some(picker.query.clone());
                DialogState::QuickOpen(picker)
            }
            Dialog::PickBranch { rebase } => {
                let picker = pickers::Picker::new(self, "Filter branches", window, cx);
                focus = Some(picker.query.clone());
                DialogState::PickBranch { rebase, picker }
            }
            Dialog::RepoSettings(id) => DialogState::RepoSettings(id),
            Dialog::Profiles(selected) => {
                DialogState::Profiles(profiles::ProfilesForm::new(self, selected, window, cx))
            }
            Dialog::ForgeSettings(id) => {
                let form = pulls::ForgeSettingsForm::new(self, id, window, cx);
                focus = Some(form.token.clone());
                DialogState::ForgeSettings(form)
            }
            Dialog::CreatePr => {
                let form = pulls::CreatePrForm::new(self, window, cx);
                for input in [&form.title, &form.base] {
                    self.watch_input(input, window, cx);
                }
                let body = form.body.clone();
                let sub = cx.subscribe_in(&body, window, |_, _, _, _, cx| cx.notify());
                self.dialog_subscriptions.push(sub);
                focus = Some(form.title.clone());
                DialogState::CreatePr(form)
            }
            Dialog::PrReview { number, event } => {
                let form = pulls::ReviewForm::new(number, event, window, cx);
                let sub = cx.subscribe_in(&form.body, window, |_, _, _, _, cx| cx.notify());
                self.dialog_subscriptions.push(sub);
                focus = Some(form.body.clone());
                DialogState::PrReview(form)
            }
            Dialog::PrMerge { number } => DialogState::PrMerge(pulls::MergeForm::new(self, number)),
        };
        self.dialog = Some(state);
        match focus {
            Some(input) => input.update(cx, |input, cx| input.focus(window, cx)),
            None => self.focus_root(window, cx),
        }
        cx.notify();
    }

    pub fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(DialogState::Clone(form)) = &self.dialog
            && form.running.is_some()
        {
            return;
        }
        self.dialog = None;
        self.dialog_subscriptions.clear();
        self.focus_root(window, cx);
        cx.notify();
    }

    fn run_and_close(&mut self, op: RepoOp, window: &mut Window, cx: &mut Context<Self>) {
        self.close_dialog(window, cx);
        self.active_op(op, cx);
    }

    /// Primary action of the open dialog (button or Enter in a field).
    pub fn submit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialog else { return };
        match dialog {
            DialogState::Confirm { op, .. } => {
                let op = op.clone();
                self.run_and_close(op, window, cx);
            }
            DialogState::Prompt {
                kind,
                text,
                message,
                checked,
            } => {
                let text = text.read(cx).text().to_owned();
                let message = message
                    .as_ref()
                    .map(|m| m.read(cx).text().to_owned())
                    .unwrap_or_default();
                if let Some(op) = kind.build(&text, &message, *checked) {
                    self.run_and_close(op, window, cx);
                }
            }
            DialogState::Reset { target, mode, .. } => {
                let op = RepoOp::Reset {
                    target: target.clone(),
                    mode: *mode,
                };
                self.run_and_close(op, window, cx);
            }
            DialogState::AddRemote { name, url } => {
                let name = name.read(cx).text().trim().to_owned();
                let url = url.read(cx).text().trim().to_owned();
                if !name.is_empty() && !url.is_empty() {
                    self.run_and_close(RepoOp::AddRemote { name, url }, window, cx);
                }
            }
            DialogState::Clone(_) => self.start_clone(cx),
            DialogState::QuickOpen(_) => self.quick_open_pick(None, window, cx),
            DialogState::PickBranch { .. } => self.pick_branch(None, window, cx),
            DialogState::ForgeSettings(form) => {
                let token = form.token.read(cx).text().to_owned();
                let id = form.id;
                let host = self
                    .state
                    .view(id)
                    .prs
                    .info
                    .ready()
                    .map(|i| i.repo.host.clone());
                if let Some(host) = host.filter(|_| !token.trim().is_empty()) {
                    self.close_dialog(window, cx);
                    self.save_forge_token(id, host, token, cx);
                }
            }
            DialogState::CreatePr(form) => {
                if let Some(new) = form.build(cx).filter(|_| !form.head.is_empty()) {
                    self.close_dialog(window, cx);
                    self.create_pull_request(new, cx);
                }
            }
            DialogState::PrReview(form) => {
                if form.valid(cx) {
                    let (number, event) = (form.number, form.event);
                    let body = form.body.read(cx).text().to_owned();
                    self.close_dialog(window, cx);
                    self.submit_review(number, event, body, cx);
                }
            }
            DialogState::PrMerge(form) => {
                if let Some(method) = form.method {
                    let number = form.number;
                    self.close_dialog(window, cx);
                    self.merge_pull_request(number, method, cx);
                }
            }
            DialogState::RepoSettings(_) | DialogState::Profiles(_) => {}
        }
    }
}

fn confirm_footer(
    label: String,
    destructive: bool,
    disabled: bool,
    cx: &mut Context<GitView>,
) -> [AnyElement; 2] {
    [
        ghost_button("dialog-cancel", "Cancel")
            .on_click(cx.listener(|app, _, window, cx| app.close_dialog(window, cx)))
            .into_any_element(),
        confirm_button("dialog-confirm", label, destructive)
            .disabled(disabled)
            .on_click(cx.listener(|app, _, window, cx| app.submit_dialog(window, cx)))
            .into_any_element(),
    ]
}

fn modal(title: impl Into<gpui::SharedString>, cx: &mut Context<GitView>) -> Modal {
    Modal::new(title).on_dismiss(cx.listener(|app, _, window, cx| app.close_dialog(window, cx)))
}

pub fn render(
    app: &mut GitView,
    window: &mut Window,
    cx: &mut Context<GitView>,
) -> Option<AnyElement> {
    let dialog = app.dialog.as_ref()?;
    let element = match dialog {
        DialogState::Confirm {
            title,
            body,
            confirm,
            destructive,
            ..
        } => {
            let [cancel, ok] = confirm_footer(confirm.clone(), *destructive, false, cx);
            modal(title.clone(), cx)
                .child(Label::new(body.clone()))
                .footer(cancel)
                .footer(ok)
                .into_any_element()
        }
        DialogState::Prompt {
            kind,
            text,
            message,
            checked,
        } => {
            let (title, field_label, message_label, check) = kind.labels();
            let subtitle = match kind {
                PromptKind::CreateBranch { start: Some(s) } => format!("Starting at {}", short(s)),
                PromptKind::CreateBranch { start: None } => "Starting at HEAD".to_owned(),
                PromptKind::RenameBranch { old } => format!("Renaming “{old}”"),
                PromptKind::CreateTag { target } => format!("At {}", short(target)),
                PromptKind::CheckoutRemoteAs { remote_branch } => {
                    format!("A local branch with that name exists; track {remote_branch} as:")
                }
                PromptKind::StashSave => String::new(),
            };
            let valid = *kind == PromptKind::StashSave || !text.read(cx).text().trim().is_empty();
            let checked = *checked;
            let [cancel, ok] = confirm_footer(title.to_owned(), false, !valid, cx);
            modal(title, cx)
                .when(!subtitle.is_empty(), |m| m.child(hint(subtitle)))
                .child(field(field_label, text.clone()))
                .when_some(message_label.zip(message.clone()), |m, (label, input)| {
                    m.child(field(label, input))
                })
                .when_some(check, |m, (label, _)| {
                    m.child(
                        Checkbox::new("prompt-check", ToggleState::from(checked))
                            .label(label)
                            .label_size(LabelSize::Small)
                            .on_click(cx.listener(|app, state: &ToggleState, _, cx| {
                                if let Some(DialogState::Prompt { checked, .. }) = &mut app.dialog {
                                    *checked = state.selected();
                                }
                                cx.notify();
                            })),
                    )
                })
                .footer(cancel)
                .footer(ok)
                .into_any_element()
        }
        DialogState::Reset { label, mode, .. } => {
            let current = *mode;
            let modes = [
                (ResetMode::Soft, "Soft", "Keep all changes staged."),
                (
                    ResetMode::Mixed,
                    "Mixed",
                    "Keep changes in the working copy, unstaged.",
                ),
                (
                    ResetMode::Hard,
                    "Hard",
                    "Discard all changes. This cannot be undone.",
                ),
            ];
            let rows: Vec<AnyElement> = modes
                .into_iter()
                .map(|(m, name, hint_text)| {
                    radio_row(
                        name,
                        current == m,
                        name,
                        Some(hint_text.into()),
                        cx.listener(move |app, _, _, cx| {
                            if let Some(DialogState::Reset { mode, .. }) = &mut app.dialog {
                                *mode = m;
                            }
                            cx.notify();
                        }),
                        cx,
                    )
                    .into_any_element()
                })
                .collect();
            let [cancel, ok] =
                confirm_footer("Reset".into(), current == ResetMode::Hard, false, cx);
            modal(format!("Reset current branch to {label}?"), cx)
                .child(v_flex().gap_1().children(rows))
                .footer(cancel)
                .footer(ok)
                .into_any_element()
        }
        DialogState::AddRemote { name, url } => {
            let valid =
                !name.read(cx).text().trim().is_empty() && !url.read(cx).text().trim().is_empty();
            let [cancel, ok] = confirm_footer("Add remote".into(), false, !valid, cx);
            modal("Add remote", cx)
                .child(field("Name", name.clone()))
                .child(field("URL", url.clone()))
                .child(hint(
                    "SSH URLs authenticate with this repository's profile key.",
                ))
                .footer(cancel)
                .footer(ok)
                .into_any_element()
        }
        DialogState::Clone(_) => clone::render(app, cx),
        DialogState::QuickOpen(_) => pickers::render_quick_open(app, cx),
        DialogState::PickBranch { .. } => pickers::render_pick_branch(app, cx),
        DialogState::RepoSettings(id) => {
            let id = *id;
            repo_settings::render(app, id, window, cx)
        }
        DialogState::Profiles(_) => profiles::render(app, cx),
        DialogState::ForgeSettings(form) => pulls::render_forge_settings(app, form, cx),
        DialogState::CreatePr(form) => pulls::render_create(app, form, cx),
        DialogState::PrReview(form) => pulls::render_review(form, cx),
        DialogState::PrMerge(form) => pulls::render_merge(form, cx),
    };
    Some(element)
}

pub const WIDE: f32 = 672.;
pub const WIDEST: f32 = 896.;

pub fn width(w: f32) -> gpui::Pixels {
    px(w)
}
