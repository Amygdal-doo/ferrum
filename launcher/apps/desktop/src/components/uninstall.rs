//! Uninstalling an app: the confirmation dialog, opened from the app page's
//! Uninstall button and the Uninstall… entry of a row's menu.

use crate::app::FerrumLauncher;
use ferrum_ui::components::{Checkbox, TintColor};
use ferrum_ui::prelude::*;
use ferrumlauncher_core::AppId;
use std::path::Path;

/// The open confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UninstallDialog {
    pub app: AppId,
    /// "Also remove settings and data" is ticked.
    pub remove_data: bool,
}

/// Whether `app` can be uninstalled from the launcher: installed, and not
/// the launcher itself.
pub fn can_uninstall(launcher: &FerrumLauncher, app: AppId) -> bool {
    app != AppId::LAUNCHER && !launcher.state.app(app).installed.is_empty()
}

/// `~/…` for paths in the home folder, for display.
fn display_path(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

impl FerrumLauncher {
    /// Asks before uninstalling `app`.
    pub fn confirm_uninstall(&mut self, app: AppId, cx: &mut Context<Self>) {
        if !can_uninstall(self, app) {
            return;
        }
        self.uninstall_dialog = Some(UninstallDialog {
            app,
            remove_data: false,
        });
        cx.notify();
    }

    pub fn close_uninstall_dialog(&mut self, cx: &mut Context<Self>) {
        self.uninstall_dialog = None;
        cx.notify();
    }

    /// Removes every version of `app` (and, if asked, its settings and data).
    pub fn uninstall_app(&mut self, app: AppId, remove_data: bool, cx: &mut Context<Self>) {
        self.uninstall_dialog = None;
        let message = format!("Uninstalled {}.", self.state.info(app).name);
        self.update_app_state(
            app,
            Some(message),
            move |state, paths| state.uninstall_app(app, remove_data, paths),
            cx,
        );
    }

    pub fn render_uninstall_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.uninstall_dialog?;
        let app = dialog.app;
        let name = self.state.info(app).name;
        let removal = self.state.removal(app, &self.services.paths);
        let versions = match removal.versions.as_slice() {
            [only] => format!("version {only}"),
            versions => format!("{} versions", versions.len()),
        };
        let item = |text: String| {
            Label::new(format!("• {text}"))
                .size(LabelSize::Small)
                .color(Color::Muted)
        };
        let removed = v_flex()
            .gap_0p5()
            .when_some(removal.application.as_deref(), |this, application| {
                this.child(item(format!("the app: {}", display_path(application))))
            })
            .child(item(format!(
                "the installed {versions}: {}",
                display_path(&removal.versions_dir)
            )))
            .child(item("its command-line links and shortcuts".into()));
        let data = if removal.data_dirs.is_empty() {
            "Its settings and data in ~/.ferrum are kept.".to_string()
        } else {
            let dirs: Vec<String> = removal.data_dirs.iter().map(|d| display_path(d)).collect();
            format!(
                "Its settings and data ({}) are kept unless you tick the box below.",
                dirs.join(", ")
            )
        };
        let checked = if dialog.remove_data {
            ToggleState::Selected
        } else {
            ToggleState::Unselected
        };
        let modal = Modal::new(format!("Uninstall {name}?"))
            .on_dismiss(cx.listener(|this, _, _, cx| this.close_uninstall_dialog(cx)))
            .child(Label::new(format!(
                "This removes {name} from this computer:"
            )))
            .child(removed)
            .child(Label::new(data).size(LabelSize::Small).color(Color::Muted))
            .when(!removal.data_dirs.is_empty(), |this| {
                this.child(
                    Checkbox::new("uninstall-remove-data", checked)
                        .label("Also remove settings and data")
                        .on_click(cx.listener(|this, state: &ToggleState, _, cx| {
                            if let Some(dialog) = &mut this.uninstall_dialog {
                                dialog.remove_data = state.selected();
                            }
                            cx.notify();
                        })),
                )
            })
            .footer(
                Button::new("uninstall-cancel", "Cancel")
                    .on_click(cx.listener(|this, _, _, cx| this.close_uninstall_dialog(cx))),
            )
            .footer(
                Button::new("uninstall-confirm", "Uninstall")
                    .style(ButtonStyle::Tinted(TintColor::Error))
                    .start_icon(Icon::new(IconName::Trash))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let remove_data = this
                            .uninstall_dialog
                            .is_some_and(|dialog| dialog.remove_data);
                        this.uninstall_app(app, remove_data, cx);
                    })),
            );
        Some(modal.into_any_element())
    }
}
