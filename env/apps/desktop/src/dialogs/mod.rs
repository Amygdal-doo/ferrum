//! Modal dialogs. One is open at a time, stored in
//! [`crate::unlocked::Unlocked::dialog`], so locking drops it together
//! with everything else decrypted. Enter in any single-line field runs
//! the dialog's main action.

mod compare;
mod dotenv;
mod import;
mod project;
mod settings;
mod share;
mod variable;

use ferrum_ui::prelude::*;

use crate::app::EnvApp;
use crate::widgets::{error_text, primary_button};

pub(crate) use compare::CompareDialog;
pub(crate) use dotenv::{ExportDotenvDialog, ImportDotenvDialog};
pub(crate) use import::ImportBundleDialog;
pub(crate) use project::{EnvironmentNameDialog, NewProjectDialog};
pub(crate) use settings::SettingsDialog;
pub(crate) use share::{ShareDialog, ShareKeyDialog};
pub(crate) use variable::VariableDialog;

pub(crate) enum Dialog {
    NewProject(NewProjectDialog),
    EnvironmentName(EnvironmentNameDialog),
    Variable(VariableDialog),
    Confirm(ConfirmAction),
    ShareKey(ShareKeyDialog),
    Share(ShareDialog),
    ImportBundle(ImportBundleDialog),
    ImportDotenv(ImportDotenvDialog),
    ExportDotenv(ExportDotenvDialog),
    Compare(CompareDialog),
    Settings(SettingsDialog),
}

/// Destructive actions that ask first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmAction {
    DeleteVariable(usize),
    DeleteEnvironment(usize),
    DeleteProject(usize),
    RotateKey,
}

/// A text field whose Enter runs the open dialog's main action.
pub(crate) fn input(
    window: &mut Window,
    cx: &mut Context<EnvApp>,
    subscriptions: &mut Vec<Subscription>,
    configure: impl FnOnce(TextInput) -> TextInput,
) -> Entity<TextInput> {
    let input = cx.new(|cx| configure(TextInput::new(window, cx)));
    subscriptions.push(cx.subscribe_in(
        &input,
        window,
        |this: &mut EnvApp, _, event, window, cx| match event {
            TextInputEvent::Submit => this.submit_dialog(window, cx),
            TextInputEvent::Changed => this.dialog_input_changed(cx),
            TextInputEvent::Blurred => {}
        },
    ));
    input
}

impl EnvApp {
    pub(crate) fn open_dialog(
        &mut self,
        dialog: Dialog,
        subscriptions: Vec<Subscription>,
        focus: Option<Entity<TextInput>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(unlocked) = self.vault_mut() else {
            return;
        };
        unlocked.dialog = Some(dialog);
        unlocked.dialog_error = None;
        unlocked.dialog_subscriptions = subscriptions;
        if let Some(input) = focus {
            let handle = input.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        }
        cx.notify();
    }

    pub(crate) fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(unlocked) = self.vault_mut() {
            unlocked.dialog = None;
            unlocked.dialog_error = None;
            unlocked.dialog_subscriptions.clear();
        }
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(crate) fn dialog_mut(&mut self) -> Option<&mut Dialog> {
        self.vault_mut().and_then(|u| u.dialog.as_mut())
    }

    pub(crate) fn set_dialog_error(
        &mut self,
        message: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        if let Some(unlocked) = self.vault_mut() {
            unlocked.dialog_error = Some(message.into());
        }
        cx.notify();
    }

    fn dialog_input_changed(&mut self, cx: &mut Context<Self>) {
        match self.dialog_mut() {
            Some(Dialog::ImportBundle(d)) => d.preview = None,
            Some(Dialog::ExportDotenv(d)) => d.confirm_overwrite = false,
            _ => {}
        }
        cx.notify();
    }

    /// The open dialog's main action (Enter).
    pub(crate) fn submit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.vault().and_then(|u| u.dialog.as_ref()) else {
            return;
        };
        match dialog {
            Dialog::NewProject(_) => self.submit_new_project(window, cx),
            Dialog::EnvironmentName(_) => self.submit_environment_name(window, cx),
            Dialog::Variable(_) => self.submit_variable(window, cx),
            Dialog::Confirm(action) => self.run_confirmed(*action, window, cx),
            Dialog::ShareKey(_) => self.submit_received_key(cx),
            Dialog::Share(_) => self.create_bundle(cx),
            Dialog::ImportBundle(d) => {
                if d.preview.is_some() {
                    self.import_bundle(window, cx);
                } else {
                    self.preview_bundle(cx);
                }
            }
            Dialog::ImportDotenv(_) => self.import_dotenv(window, cx),
            Dialog::ExportDotenv(_) => self.export_dotenv(window, cx),
            Dialog::Compare(_) => self.close_dialog(window, cx),
            Dialog::Settings(_) => self.change_password(cx),
        }
    }

    pub(crate) fn render_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let unlocked = self.vault()?;
        let dialog = unlocked.dialog.as_ref()?;
        let modal = match dialog {
            Dialog::NewProject(d) => self.render_new_project(d, cx),
            Dialog::EnvironmentName(d) => self.render_environment_name(d, cx),
            Dialog::Variable(d) => self.render_variable(d, cx),
            Dialog::Confirm(action) => self.render_confirm(*action, cx),
            Dialog::ShareKey(d) => self.render_share_key(d, cx),
            Dialog::Share(d) => self.render_share(d, cx),
            Dialog::ImportBundle(d) => self.render_import_bundle(d, cx),
            Dialog::ImportDotenv(d) => self.render_import_dotenv(d, cx),
            Dialog::ExportDotenv(d) => self.render_export_dotenv(d, cx),
            Dialog::Compare(d) => self.render_compare(d, cx),
            Dialog::Settings(d) => self.render_settings(d, window, cx),
        };
        let modal = match &unlocked.dialog_error {
            Some(error) => modal.child(error_text(error.clone())),
            None => modal,
        };
        Some(modal.into_any_element())
    }

    /// A dialog frame whose close button (and Escape) dismisses it.
    pub(crate) fn modal(&self, title: impl Into<SharedString>, cx: &mut Context<Self>) -> Modal {
        Modal::new(title)
            .on_dismiss(cx.listener(|this, _, window, cx| this.close_dialog(window, cx)))
    }

    pub(crate) fn confirm(
        &mut self,
        action: ConfirmAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_dialog(Dialog::Confirm(action), Vec::new(), None, window, cx);
    }

    fn render_confirm(&self, action: ConfirmAction, cx: &mut Context<Self>) -> Modal {
        let unlocked = self.vault();
        let project = unlocked.and_then(|u| u.project());
        let env = unlocked.and_then(|u| u.environment());
        let (title, message, button) = match action {
            ConfirmAction::DeleteVariable(i) => (
                "Delete variable",
                format!(
                    "Delete {} from {}?",
                    env.and_then(|e| e.variables.get(i)).map_or("", |v| v.key.as_str()),
                    env.map_or("", |e| e.name.as_str())
                ),
                "Delete",
            ),
            ConfirmAction::DeleteEnvironment(i) => (
                "Delete environment",
                format!(
                    "Delete the environment {} and all its variables?",
                    project
                        .and_then(|p| p.environments.get(i))
                        .map_or("", |e| e.name.as_str())
                ),
                "Delete",
            ),
            ConfirmAction::DeleteProject(i) => (
                "Delete project",
                format!(
                    "Delete {} with all its environments, variables and its share key? This cannot be undone.",
                    unlocked
                        .and_then(|u| u.vault.projects.get(i))
                        .map_or("", |p| p.name.as_str())
                ),
                "Delete project",
            ),
            ConfirmAction::RotateKey => (
                "Generate a new share key",
                "Teammates will need the new key for bundles you send from now on, and bundles made with the old key can no longer be opened here.".to_string(),
                "Generate new key",
            ),
        };
        self.modal(title, cx)
            .child(Label::new(message))
            .footer(
                Button::new("confirm-cancel", "Cancel")
                    .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            )
            .footer(primary_button(
                "confirm-ok",
                button,
                cx,
                move |this, window, cx| {
                    this.run_confirmed(action, window, cx);
                },
            ))
    }

    fn run_confirmed(
        &mut self,
        action: ConfirmAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_dialog(window, cx);
        match action {
            ConfirmAction::DeleteVariable(i) => {
                self.change(cx, |u| {
                    u.environment_mut()
                        .and_then(|e| e.remove(i))
                        .map(drop)
                        .ok_or_else(|| "That variable no longer exists".to_string())
                });
            }
            ConfirmAction::DeleteEnvironment(i) => {
                self.change(cx, |u| {
                    let project = u.project_mut().ok_or("No project")?;
                    if i >= project.environments.len() {
                        return Err("That environment no longer exists".into());
                    }
                    project.environments.remove(i);
                    let last = project.environments.len().saturating_sub(1);
                    u.env = u.env.min(last);
                    Ok(())
                });
            }
            ConfirmAction::DeleteProject(i) => {
                let removed = self.change(cx, |u| {
                    if i >= u.vault.projects.len() {
                        return Err("That project no longer exists".into());
                    }
                    u.vault.projects.remove(i);
                    u.project = None;
                    Ok(u.vault.projects.len())
                });
                if let Some(remaining) = removed {
                    if remaining > 0 {
                        self.select_project(i.min(remaining - 1), window, cx);
                    } else if let Some(u) = self.vault() {
                        let bar = u.bar.clone();
                        bar.update(cx, |bar, cx| {
                            bar.set_project(None, cx);
                            bar.set_detail(None, cx);
                        });
                    }
                }
            }
            ConfirmAction::RotateKey => self.generate_share_key(window, cx),
        }
    }
}
