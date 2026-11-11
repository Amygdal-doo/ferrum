//! `.env` interop: import a project folder's `.env*` files (or any file),
//! and write an environment to a `.env` file (confirming an overwrite and
//! offering to add it to `.gitignore`).

use std::path::{Path, PathBuf};

use ferrum_ui::gpui::PathPromptOptions;
use ferrum_ui::prelude::*;
use ferrum_ui::ui::Checkbox;
use ferrumenv_core::dotenv::{self, ParsedEnv};

use super::{Dialog, input};
use crate::app::EnvApp;
use crate::widgets::{field, muted, primary_button, text_button};

pub(crate) struct ImportDotenvDialog {
    pub(crate) files: Vec<DotenvFile>,
}

pub(crate) struct DotenvFile {
    pub(crate) path: PathBuf,
    pub(crate) environment: String,
    pub(crate) checked: bool,
    pub(crate) parsed: ParsedEnv,
}

impl DotenvFile {
    fn read(path: PathBuf) -> Result<Self, String> {
        let text = zeroize::Zeroizing::new(
            std::fs::read_to_string(&path)
                .map_err(|e| format!("Could not read {}: {e}", path.display()))?,
        );
        let name = file_name(&path);
        Ok(Self {
            environment: dotenv::environment_for_file(&name),
            // Example files hold placeholders, not values.
            checked: !name.contains("example") && !name.contains("sample"),
            parsed: dotenv::parse(&text),
            path,
        })
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) struct ExportDotenvDialog {
    pub(crate) path: Entity<TextInput>,
    /// The file exists and the user has been warned once.
    pub(crate) confirm_overwrite: bool,
    pub(crate) add_to_gitignore: bool,
}

impl EnvApp {
    pub(crate) fn open_import_dotenv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.vault().and_then(|u| u.project()) else {
            return;
        };
        let files = project
            .path
            .as_deref()
            .map(dotenv::find_env_files)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|path| DotenvFile::read(path).ok())
            .collect();
        let dialog = ImportDotenvDialog { files };
        self.open_dialog(Dialog::ImportDotenv(dialog), Vec::new(), None, window, cx);
    }

    /// Adds a file to the import list (used by "Choose file…" and tests).
    pub(crate) fn add_dotenv_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match DotenvFile::read(path) {
            Ok(file) => {
                if let Some(Dialog::ImportDotenv(d)) = self.dialog_mut() {
                    d.files.retain(|f| f.path != file.path);
                    d.files.push(file);
                }
                cx.notify();
            }
            Err(message) => self.set_dialog_error(message, cx),
        }
    }

    fn choose_dotenv_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Import".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            this.update(cx, |this, cx| {
                for path in paths {
                    this.add_dotenv_file(path, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn import_dotenv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let done = self.change(cx, |u| {
            if u.project().is_none() {
                return Err("No project selected".into());
            }
            let Some(Dialog::ImportDotenv(d)) = u.dialog.take() else {
                return Err("Nothing to import".into());
            };
            let mut count = 0;
            if let Some(project) = u.project_mut() {
                for file in d.files.iter().filter(|f| f.checked) {
                    let env = project.environment_mut(&file.environment);
                    for var in &file.parsed.variables {
                        env.upsert(var.clone());
                        count += 1;
                    }
                }
            }
            if count == 0 {
                // Keep the dialog open to show the error.
                u.dialog = Some(Dialog::ImportDotenv(d));
                return Err("Tick at least one file with variables.".into());
            }
            Ok(count)
        });
        if let Some(count) = done {
            self.close_dialog(window, cx);
            self.show_info(format!("Imported {count} variables."), cx);
        }
    }

    pub(crate) fn render_import_dotenv(
        &self,
        d: &ImportDotenvDialog,
        cx: &mut Context<Self>,
    ) -> Modal {
        let mut list = v_flex().gap_1();
        if d.files.is_empty() {
            list = list.child(muted(
                "No .env files found in the project folder. Choose a file instead.",
            ));
        }
        for (i, file) in d.files.iter().enumerate() {
            let warnings = file.parsed.warnings.len();
            let detail = format!(
                "→ {} · {} variables{}",
                file.environment,
                file.parsed.variables.len(),
                if warnings > 0 {
                    format!(" · {warnings} lines skipped")
                } else {
                    String::new()
                }
            );
            list = list.child(
                h_flex()
                    .gap_2()
                    .child(
                        Checkbox::new(
                            SharedString::from(format!("dotenv-{i}")),
                            ToggleState::from(file.checked),
                        )
                        .label(file_name(&file.path))
                        .label_size(LabelSize::Small)
                        .on_click(cx.listener(
                            move |this, _: &ToggleState, _, cx| {
                                if let Some(Dialog::ImportDotenv(d)) = this.dialog_mut()
                                    && let Some(file) = d.files.get_mut(i)
                                {
                                    file.checked = !file.checked;
                                }
                                cx.notify();
                            },
                        )),
                    )
                    .child(muted(detail)),
            );
        }
        self.modal("Import .env files", cx)
            .width(px(560.))
            .child(muted(
                "Variables are added to the matching environment (.env → development, .env.production → production); existing keys are overwritten.",
            ))
            .child(list)
            .footer(text_button("choose-dotenv", "Choose file…", cx, |this, window, cx| {
                this.choose_dotenv_file(window, cx);
            }))
            .footer(primary_button("import-dotenv-ok", "Import", cx, |this, window, cx| {
                this.import_dotenv(window, cx);
            }))
    }

    pub(crate) fn open_export_dotenv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(u) = self.vault() else {
            return;
        };
        let (Some(project), Some(env)) = (u.project(), u.environment()) else {
            return;
        };
        let default = project
            .path
            .as_ref()
            .map(|dir| {
                dir.join(dotenv::file_for_environment(&env.name))
                    .display()
                    .to_string()
            })
            .unwrap_or_default();
        let mut subscriptions = Vec::new();
        let path = input(window, cx, &mut subscriptions, |t| {
            t.monospace()
                .placeholder("/path/to/.env")
                .with_text(default)
        });
        let dialog = ExportDotenvDialog {
            path: path.clone(),
            confirm_overwrite: false,
            add_to_gitignore: true,
        };
        self.open_dialog(
            Dialog::ExportDotenv(dialog),
            subscriptions,
            Some(path),
            window,
            cx,
        );
    }

    fn choose_export_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::ExportDotenv(d)) = self.dialog_mut() else {
            return;
        };
        let current = PathBuf::from(d.path.read(cx).text());
        let dir = current
            .parent()
            .map(Path::to_path_buf)
            .filter(|p| p.is_dir())
            .or_else(ferrum_ui::project::home_dir)
            .unwrap_or_default();
        let name = current
            .file_name()
            .map_or_else(|| ".env".to_string(), |n| n.to_string_lossy().into_owned());
        let receiver = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            this.update(cx, |this, cx| {
                if let Some(Dialog::ExportDotenv(d)) = this.dialog_mut() {
                    d.path.update(cx, |input, cx| {
                        input.set_text(path.display().to_string(), cx)
                    });
                    // The system dialog already asked about replacing.
                    d.confirm_overwrite = true;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Whether `path`'s folder has a `.gitignore` that misses its file name.
    fn gitignore_misses(path: &Path) -> bool {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return false;
        };
        let ignore = std::fs::read_to_string(dir.join(".gitignore")).unwrap_or_default();
        let in_repo = ferrum_ui::project::git_root(dir).is_some() || !ignore.is_empty();
        in_repo && !dotenv::gitignore_covers(&ignore, &name.to_string_lossy())
    }

    pub(crate) fn export_dotenv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(u) = self.vault() else {
            return;
        };
        let (Some(env), Some(Dialog::ExportDotenv(d))) = (u.environment(), u.dialog.as_ref())
        else {
            return;
        };
        let path = PathBuf::from(d.path.read(cx).text().trim());
        let text = zeroize::Zeroizing::new(dotenv::to_dotenv(&env.variables));
        let confirmed = d.confirm_overwrite;
        let gitignore = d.add_to_gitignore && Self::gitignore_misses(&path);
        if path.as_os_str().is_empty() || path.file_name().is_none() {
            self.set_dialog_error("Choose where to write the file.", cx);
            return;
        }
        if path.exists() && !confirmed {
            if let Some(Dialog::ExportDotenv(d)) = self.dialog_mut() {
                d.confirm_overwrite = true;
            }
            self.set_dialog_error(
                format!(
                    "{} already exists. Click Overwrite to replace it.",
                    path.display()
                ),
                cx,
            );
            return;
        }
        if let Err(e) = ferrumenv_core::write_private(&path, text.as_bytes()) {
            self.set_dialog_error(format!("Could not write {}: {e}", path.display()), cx);
            return;
        }
        let mut message = format!("Wrote {}", path.display());
        if gitignore && let (Some(dir), Some(name)) = (path.parent(), path.file_name()) {
            match dotenv::add_to_gitignore(dir, &name.to_string_lossy()) {
                Ok(true) => message.push_str(" and added it to .gitignore"),
                Ok(false) => {}
                Err(e) => message.push_str(&format!(" (could not update .gitignore: {e})")),
            }
        }
        self.close_dialog(window, cx);
        self.show_info(message, cx);
    }

    pub(crate) fn render_export_dotenv(
        &self,
        d: &ExportDotenvDialog,
        cx: &mut Context<Self>,
    ) -> Modal {
        let env_name = self
            .vault()
            .and_then(|u| u.environment())
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let path = PathBuf::from(d.path.read(cx).text().trim());
        let offer_gitignore = Self::gitignore_misses(&path);
        let add = d.add_to_gitignore;
        self.modal(format!("Write {env_name} to a .env file"), cx)
            .width(px(560.))
            .child(field(
                "File",
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(d.path.clone()))
                    .child(text_button(
                        "choose-export",
                        "Choose…",
                        cx,
                        |this, window, cx| {
                            this.choose_export_path(window, cx);
                        },
                    )),
            ))
            .child(muted(
                "The file is written in plain text with owner-only permissions (0600).",
            ))
            .when(offer_gitignore, |m| {
                m.child(
                    Checkbox::new("gitignore", ToggleState::from(add))
                        .label("This file isn't in .gitignore: add it")
                        .label_size(LabelSize::Small)
                        .on_click(cx.listener(|this, _: &ToggleState, _, cx| {
                            if let Some(Dialog::ExportDotenv(d)) = this.dialog_mut() {
                                d.add_to_gitignore = !d.add_to_gitignore;
                            }
                            cx.notify();
                        })),
                )
            })
            .footer(primary_button(
                "export-dotenv-ok",
                if d.confirm_overwrite {
                    "Overwrite"
                } else {
                    "Write file"
                },
                cx,
                |this, window, cx| this.export_dotenv(window, cx),
            ))
    }
}
