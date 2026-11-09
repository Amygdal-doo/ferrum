//! Copying results to the clipboard and exporting them to files.

use super::{background, log_dropped};
use crate::app::FerrumData;
use ferrum_ui::gpui::ClipboardItem;
use ferrum_ui::prelude::*;
use ferrumdata_result_grid_core::ExportFormat;
use std::path::PathBuf;

impl FerrumData {
    /// Copies the active result's selection (or every row) in `format`.
    pub fn copy_results(&mut self, format: ExportFormat, cx: &mut Context<Self>) {
        let Some(run) = self
            .state
            .active_tab
            .and_then(|id| self.state.runs.get(&id))
        else {
            return;
        };
        let selection = run.selection.is_some();
        let Some((rows, _)) = run.scope(selection) else {
            self.notify_user("Nothing to copy.", false, cx);
            return;
        };
        let Some(text) = run.export_text(format, selection) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.notify_user(
            format!("Copied {} as {}.", plural_rows(rows.len()), format.label()),
            false,
            cx,
        );
    }

    /// Asks for a file and writes every loaded row of the active result, in
    /// display order. The text is captured when the menu item is chosen.
    pub fn export_results(&mut self, format: ExportFormat, cx: &mut Context<Self>) {
        let Some(tab) = self.state.active_tab() else {
            return;
        };
        let file_name = format!("{}.{}", file_stem(&tab.title), format.extension());
        let Some(run) = self.state.runs.get(&tab.id) else {
            return;
        };
        let rows = run.buffer.len();
        let truncated = run.buffer.is_truncated();
        let Some(text) = run.export_text(format, false) else {
            self.notify_user("Nothing to export.", false, cx);
            return;
        };
        let receiver = cx.prompt_for_new_path(&default_export_dir(), Some(&file_name));
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) | Err(_) => return,
                Ok(Err(e)) => {
                    log_dropped(this.update(cx, |this, cx| {
                        this.notify_user(format!("Save dialog failed: {e}"), true, cx);
                    }));
                    return;
                }
            };
            let target = path.clone();
            let written = background(
                &services,
                async move { tokio::fs::write(&target, text).await },
            )
            .await;
            log_dropped(this.update(cx, |this, cx| match written {
                Ok(Ok(())) => {
                    let limit = if truncated {
                        " (client fetch limit reached: not the full table)"
                    } else {
                        ""
                    };
                    this.notify_user(
                        format!(
                            "Exported {} to {}{limit}.",
                            plural_rows(rows),
                            path.display()
                        ),
                        false,
                        cx,
                    );
                }
                Ok(Err(e)) => this.notify_user(format!("Export failed: {e}"), true, cx),
                Err(e) => this.notify_user(format!("Export failed: {e}"), true, cx),
            }));
        })
        .detach();
    }
}

fn plural_rows(n: usize) -> String {
    if n == 1 {
        "1 row".into()
    } else {
        format!("{n} rows")
    }
}

/// Tab title made safe as a file name.
fn file_stem(title: &str) -> String {
    let stem: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let stem = stem.trim().trim_matches('.');
    if stem.is_empty() {
        "results".into()
    } else {
        stem.to_owned()
    }
}

fn default_export_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|dirs| {
            dirs.download_dir()
                .map_or_else(|| dirs.home_dir().to_path_buf(), PathBuf::from)
        })
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::{file_stem, plural_rows};

    #[test]
    fn file_names_come_from_tab_titles() {
        assert_eq!(file_stem("SQL 1"), "SQL 1");
        assert_eq!(file_stem("we/ird:name"), "we_ird_name");
        assert_eq!(file_stem("../.."), "_");
        assert_eq!(file_stem("  "), "results");
        assert_eq!(plural_rows(1), "1 row");
        assert_eq!(plural_rows(3), "3 rows");
    }
}
