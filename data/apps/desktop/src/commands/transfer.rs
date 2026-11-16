//! SQL dump and CSV import tabs: loading what they need, choosing files and
//! running them on the services runtime.

use super::export::{default_export_dir, file_stem};
use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::{DumpView, ImportView, SnapshotLoad, TaskStatus, ToolTab};
use ferrum_ui::gpui::PathPromptOptions;
use ferrum_ui::prelude::*;
use ferrumdata_application::dump::{DumpOptions, write_dump};
use ferrumdata_application::import::{auto_map, parse_csv};
use ferrumdata_domain::{EditorTabId, ProfileId, TableKind, TableRef};
use std::path::PathBuf;
use std::sync::Arc;

/// Largest CSV file read into memory for an import.
const MAX_CSV_BYTES: u64 = 256 * 1024 * 1024;

impl FerrumData {
    fn tab_profile(&self, tab_id: EditorTabId) -> Option<ProfileId> {
        self.state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.profile_id)
    }

    pub fn open_dump(&mut self, profile_id: ProfileId, cx: &mut Context<Self>) {
        let name = self
            .state
            .profile(profile_id)
            .map_or_else(String::new, |p| p.display_name.clone());
        let tab_id = self.open_tool(profile_id, ToolTab::Dump, format!("Dump · {name}"));
        if !self.state.dumps.contains_key(&tab_id) {
            self.load_dump_tables(tab_id, profile_id, cx);
        }
        cx.notify();
    }

    pub fn load_dump_tables(
        &mut self,
        tab_id: EditorTabId,
        profile_id: ProfileId,
        cx: &mut Context<Self>,
    ) {
        self.state.dumps.insert(tab_id, DumpView::default());
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.connections.describe_schema(profile_id).await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let Some(view) = this.state.dumps.get_mut(&tab_id) else {
                    return;
                };
                view.load = match result {
                    Ok(Ok(snapshot)) => {
                        // Views are not dumped (their definition is not read).
                        view.selected = snapshot
                            .tables
                            .iter()
                            .filter(|t| t.info.kind != TableKind::View)
                            .map(|t| t.info.table.clone())
                            .collect();
                        SnapshotLoad::Loaded(Arc::new(snapshot))
                    }
                    Ok(Err(e)) => SnapshotLoad::Failed(e.message),
                    Err(e) => SnapshotLoad::Failed(e),
                };
                cx.notify();
            }));
        })
        .detach();
    }

    /// Asks where to save, then writes the dump of the selected tables.
    pub fn run_dump(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let (Some(profile), Some(view)) = (
            self.tab_profile(tab_id)
                .and_then(|id| self.state.profile(id))
                .cloned(),
            self.state.dumps.get(&tab_id).cloned(),
        ) else {
            return;
        };
        let SnapshotLoad::Loaded(snapshot) = &view.load else {
            return;
        };
        let tables: Vec<_> = snapshot
            .tables
            .iter()
            .filter(|t| view.selected.contains(&t.info.table))
            .cloned()
            .collect();
        if tables.is_empty() || !(view.schema || view.data) {
            self.notify_user("Choose at least one table and schema or data.", true, cx);
            return;
        }
        let options = DumpOptions {
            schema: view.schema,
            data: view.data,
        };
        let file_name = format!("{}.sql", file_stem(&profile.display_name));
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
            log_dropped(this.update(cx, |this, cx| {
                if let Some(view) = this.state.dumps.get_mut(&tab_id) {
                    view.status = TaskStatus::Running;
                    cx.notify();
                }
            }));
            let queries = services.queries.clone();
            let target = path.clone();
            let result = background(&services, async move {
                write_dump(
                    &queries,
                    profile.engine,
                    profile.id,
                    &tables,
                    options,
                    &target,
                )
                .await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let status = match result {
                    Ok(Ok(summary)) => TaskStatus::Done(format!(
                        "Wrote {} tables and {} rows to {}",
                        summary.tables,
                        summary.rows,
                        path.display()
                    )),
                    Ok(Err(e)) => TaskStatus::Failed(e.message),
                    Err(e) => TaskStatus::Failed(e),
                };
                if let Some(view) = this.state.dumps.get_mut(&tab_id) {
                    view.status = status;
                }
                // Each table read was recorded in the history.
                this.invalidate_history();
                cx.notify();
            }));
        })
        .detach();
    }

    /// Opens (or focuses) the import tab of a table.
    pub fn open_import(&mut self, profile_id: ProfileId, table: TableRef, cx: &mut Context<Self>) {
        let existing = self
            .state
            .tabs
            .iter()
            .filter(|t| t.profile_id == Some(profile_id) && t.tool == Some(ToolTab::Import))
            .find(|t| {
                self.state
                    .imports
                    .get(&t.id)
                    .is_some_and(|v| v.table == table)
            })
            .map(|t| t.id);
        if let Some(id) = existing {
            self.state.active_tab = Some(id);
            cx.notify();
            return;
        }
        let tab_id = self.new_tool_tab(
            profile_id,
            ToolTab::Import,
            format!("Import · {}", table.name),
        );
        self.state
            .imports
            .insert(tab_id, ImportView::new(table.clone()));
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.connections.describe_table(profile_id, table).await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                if let Some(view) = this.state.imports.get_mut(&tab_id) {
                    view.columns = match result {
                        Ok(Ok(details)) => {
                            Ok(details.columns.into_iter().map(|c| c.name).collect())
                        }
                        Ok(Err(e)) => Err(e.message),
                        Err(e) => Err(e),
                    };
                    remap(view);
                    cx.notify();
                }
            }));
        })
        .detach();
    }

    pub fn choose_import_file(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import".into()),
        });
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let path: PathBuf = match receiver.await {
                Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                    Some(path) => path,
                    None => return,
                },
                Ok(Ok(None)) | Err(_) => return,
                Ok(Err(e)) => {
                    log_dropped(this.update(cx, |this, cx| {
                        this.notify_user(format!("File dialog failed: {e}"), true, cx);
                    }));
                    return;
                }
            };
            let source = path.clone();
            let read = background(&services, async move {
                let size = tokio::fs::metadata(&source).await?.len();
                if size > MAX_CSV_BYTES {
                    return Err(std::io::Error::other(format!(
                        "the file is {} MB; at most {} MB can be imported",
                        size / 1_048_576,
                        MAX_CSV_BYTES / 1_048_576
                    )));
                }
                let bytes = tokio::fs::read(&source).await?;
                Ok(parse_csv(&String::from_utf8_lossy(&bytes), None))
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let csv = match read {
                    Ok(Ok(csv)) => csv,
                    Ok(Err(e)) => {
                        return this.notify_user(format!("Could not read the file: {e}"), true, cx);
                    }
                    Err(e) => return this.notify_user(e, true, cx),
                };
                if let Some(view) = this.state.imports.get_mut(&tab_id) {
                    view.file = Some(path);
                    view.csv = Some(Arc::new(csv));
                    view.status = TaskStatus::Idle;
                    remap(view);
                    cx.notify();
                }
            }));
        })
        .detach();
    }

    /// Inserts the mapped rows in one transaction.
    pub fn run_import(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let Some(profile) = self
            .tab_profile(tab_id)
            .and_then(|id| self.state.profile(id))
            .cloned()
        else {
            return;
        };
        let Some(view) = self.state.imports.get(&tab_id).cloned() else {
            return;
        };
        let (Ok(columns), Some(csv)) = (&view.columns, &view.csv) else {
            return;
        };
        let script = match view
            .plan
            .statements(profile.engine, &view.table, columns, csv)
        {
            Ok(script) => script,
            Err(e) => return self.notify_user(e, true, cx),
        };
        let rows = view.plan.data(csv).len();
        if let Some(view) = self.state.imports.get_mut(&tab_id) {
            view.status = TaskStatus::Running;
        }
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.connections.execute_script(profile.id, script).await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let status = match result {
                    Ok(Ok(_)) => TaskStatus::Done(format!("Imported {rows} rows.")),
                    Ok(Err(e)) => {
                        TaskStatus::Failed(format!("{} Nothing was imported.", e.message))
                    }
                    Err(e) => TaskStatus::Failed(e),
                };
                if let Some(view) = this.state.imports.get_mut(&tab_id) {
                    view.status = status;
                }
                cx.notify();
            }));
        })
        .detach();
    }
}

/// Re-maps columns by header name after the columns or the file changed.
fn remap(view: &mut ImportView) {
    let (Ok(columns), Some(csv)) = (&view.columns, &view.csv) else {
        return;
    };
    view.plan.mapping = match csv.records.first() {
        Some(headers) if view.plan.header => auto_map(columns, headers),
        // Without a header, map by position.
        Some(first) => (0..columns.len())
            .map(|i| (i < first.len()).then_some(i))
            .collect(),
        None => vec![None; columns.len()],
    };
}

/// Toggling "first row is a header" maps again (by name or by position).
pub(crate) fn set_header(view: &mut ImportView, header: bool) {
    view.plan.header = header;
    remap(view);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_follows_the_header_choice() {
        let mut view = ImportView::new(TableRef {
            schema: None,
            name: "t".into(),
        });
        view.columns = Ok(vec!["id".into(), "name".into(), "note".into()]);
        view.csv = Some(Arc::new(parse_csv("name,id\nann,1\n", None)));
        remap(&mut view);
        assert_eq!(view.plan.mapping, vec![Some(1), Some(0), None]);
        set_header(&mut view, false);
        assert_eq!(view.plan.mapping, vec![Some(0), Some(1), None]);
    }
}
