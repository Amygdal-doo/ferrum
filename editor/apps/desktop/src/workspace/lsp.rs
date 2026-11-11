//! Language server features at the window level: jumping to definitions
//! (with back / forward history), the problems list, restarting servers
//! and the status bar's problem counts and server state.

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrumeditor_core::lsp::{Location, Position, PositionEncoding, Severity, uri_to_path};
use ui::{Icon, IconName, IconSize};

use super::Workspace;
use crate::editor::{EditorEvent, EditorView};
use crate::lsp_store::{LspStore, ServerStatus};
use crate::picker::{PickerItem, PickerKind};

/// Most places remembered for navigation:back.
const HISTORY_LIMIT: usize = 100;

/// A place to go back to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavEntry {
    pub path: PathBuf,
    pub offset: usize,
}

/// Picker payload for a file position: "path\tline\tcharacter\tencoding".
fn location_payload(path: &Path, position: Position, encoding: PositionEncoding) -> String {
    let units = match encoding {
        PositionEncoding::Utf8 => 8,
        PositionEncoding::Utf16 => 16,
    };
    format!(
        "{}\t{}\t{}\t{units}",
        path.to_string_lossy(),
        position.line,
        position.character
    )
}

fn parse_location_payload(payload: &str) -> Option<(PathBuf, Position, PositionEncoding)> {
    let mut parts = payload.rsplitn(4, '\t');
    let encoding = match parts.next()? {
        "8" => PositionEncoding::Utf8,
        _ => PositionEncoding::Utf16,
    };
    let character = parts.next()?.parse().ok()?;
    let line = parts.next()?.parse().ok()?;
    Some((
        PathBuf::from(parts.next()?),
        Position::new(line, character),
        encoding,
    ))
}

impl Workspace {
    /// Editor events about language servers (from `watch_editor`).
    pub(super) fn on_editor_lsp_event(
        &mut self,
        editor: &Entity<EditorView>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            EditorEvent::Navigate {
                locations,
                encoding,
            } => self.navigate(locations, *encoding, window, cx),
            EditorEvent::Notice(message) => self.info(message.clone(), cx),
            EditorEvent::ApplyEdit {
                edit,
                encoding,
                label,
                command,
            } => self.apply_editor_edit(editor, edit, *encoding, label, command.as_ref(), cx),
            EditorEvent::References {
                locations,
                encoding,
                symbol,
            } => self.show_references(locations, *encoding, symbol, cx),
            EditorEvent::CodeActions(actions) => self.pick_code_action(editor, actions, window, cx),
            _ => {}
        }
    }

    fn relative(&self, path: &Path) -> String {
        self.root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    }

    fn navigate(
        &mut self,
        locations: &[Location],
        encoding: PositionEncoding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let targets: Vec<(PathBuf, Position)> = locations
            .iter()
            .filter_map(|l| Some((uri_to_path(&l.uri)?, l.range.start)))
            .collect();
        match targets.as_slice() {
            [] => self.info("The definition is not in a file", cx),
            [(path, position)] => {
                self.push_history(cx);
                self.open_lsp_location(path, *position, encoding, window, cx);
            }
            many => {
                self.location_items = many
                    .iter()
                    .map(|(path, position)| {
                        let mut item = PickerItem::new(
                            format!("{}:{}", self.relative(path), position.line + 1),
                            "",
                            location_payload(path, *position, encoding),
                        );
                        item.icon = Some(path.clone());
                        item
                    })
                    .collect();
                if self.picker.is_some() {
                    self.close_picker(window, cx);
                }
                self.toggle_picker(PickerKind::Locations, window, cx);
            }
        }
    }

    fn open_lsp_location(
        &mut self,
        path: &Path,
        position: Position,
        encoding: PositionEncoding,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = self.open_file(path, window, cx) {
            editor.update(cx, |e, cx| e.go_to_lsp_position(position, encoding, cx));
        }
    }

    /// Opens a Problems / Locations picker payload.
    pub(super) fn open_location_payload(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((path, position, encoding)) = parse_location_payload(payload) {
            self.push_history(cx);
            self.open_lsp_location(&path, position, encoding, window, cx);
        }
    }

    // ----- Back / forward -----------------------------------------------------

    fn current_place(&self, cx: &App) -> Option<NavEntry> {
        let editor = self.active_editor()?;
        let editor = editor.read(cx);
        Some(NavEntry {
            path: editor.path.clone()?,
            offset: editor.state.primary().head,
        })
    }

    fn push_history(&mut self, cx: &App) {
        if let Some(place) = self.current_place(cx) {
            if self.nav_back.last() != Some(&place) {
                self.nav_back.push(place);
            }
            if self.nav_back.len() > HISTORY_LIMIT {
                self.nav_back.remove(0);
            }
            self.nav_forward.clear();
        }
    }

    pub(super) fn go_back(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        let target = if back {
            self.nav_back.pop()
        } else {
            self.nav_forward.pop()
        };
        let Some(target) = target else {
            return;
        };
        if let Some(place) = self.current_place(cx) {
            if back {
                self.nav_forward.push(place);
            } else {
                self.nav_back.push(place);
            }
        }
        if let Some(editor) = self.open_file(&target.path, window, cx) {
            editor.update(cx, |e: &mut EditorView, cx| {
                let offset = e
                    .state
                    .buffer()
                    .clip(target.offset.min(e.state.buffer().len()));
                e.state.set_cursor(offset);
                let line = e.state.buffer().row_of(offset);
                e.scroll_to_line(line, cx);
            });
        }
    }

    // ----- Problems -------------------------------------------------------------

    fn problem_items_from(&self, store: &LspStore) -> Vec<PickerItem> {
        let mut items: Vec<(Severity, PickerItem)> = Vec::new();
        for (path, encoding, list) in store.all_diagnostics() {
            let relative = self.relative(path);
            for d in list.iter().filter(|d| d.severity != Severity::Hint) {
                let message = d.message.lines().next().unwrap_or_default().to_string();
                let mut item = PickerItem::new(
                    format!("{}: {message}", d.severity.label()),
                    format!("{relative}:{}", d.range.start.line + 1),
                    location_payload(path, d.range.start, encoding),
                );
                item.icon = Some(path.clone());
                items.push((d.severity, item));
            }
        }
        // Errors first; files and lines keep their order within a severity.
        items.sort_by_key(|(severity, _)| *severity);
        items.into_iter().map(|(_, item)| item).collect()
    }

    pub(super) fn problem_items(&self, cx: &App) -> Vec<PickerItem> {
        LspStore::global(cx).map_or_else(Vec::new, |store| self.problem_items_from(store.read(cx)))
    }

    /// Installs the server for the active file's language (again, if an
    /// earlier try failed).
    pub(super) fn install_language_server(&mut self, cx: &mut Context<Self>) {
        let Some(store) = LspStore::global(cx) else {
            return;
        };
        let Some(language) = self.active_editor().map(|e| e.read(cx).language()) else {
            self.info("Open a file to install its language server", cx);
            return;
        };
        let Some(def) = ferrumeditor_core::lsp::servers::server_for(language) else {
            self.info(
                format!("There is no language server for {}", language.name()),
                cx,
            );
            return;
        };
        store.update(cx, |store, cx| store.install(def, true, cx));
    }

    pub(super) fn show_install_log(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let log = LspStore::global(cx)
            .zip(self.active_editor())
            .and_then(|(store, editor)| {
                let language = editor.read(cx).language();
                store.read(cx).install_log_for(language)
            });
        match log {
            Some(path) => {
                self.open_file(&path, window, cx);
            }
            None => self.info("No install log for this file's language server", cx),
        }
    }

    pub(super) fn on_lsp_store_event(
        &mut self,
        event: &crate::lsp_store::LspEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            crate::lsp_store::LspEvent::Notice { message, error } => {
                if *error {
                    self.error(message.clone(), cx);
                } else {
                    self.info(message.clone(), cx);
                }
            }
            crate::lsp_store::LspEvent::ApplyEdit {
                server_id,
                request_id,
                edit,
            } => self.apply_server_edit(*server_id, request_id, edit, cx),
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn restart_language_servers(&mut self, cx: &mut Context<Self>) {
        match LspStore::global(cx) {
            Some(store) => {
                store.update(cx, |store, cx| store.restart(cx));
                self.info("Restarting language servers", cx);
            }
            None => self.info(
                "Language servers are turned off ([lsp] enabled = false). Change it and reopen the window.",
                cx,
            ),
        }
    }

    // ----- Status bar -----------------------------------------------------------

    pub(super) fn render_lsp_status(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(store) = LspStore::global(cx) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let editor = self.active_editor();
        let status = editor.as_ref().and_then(|editor| {
            let e = editor.read(cx);
            store.read(cx).status_for(e.server_id(), e.language())
        });
        let store = store.read(cx);
        let (errors, warnings) = store.counts();
        if store.has_servers() || errors + warnings > 0 {
            out.push(
                h_flex()
                    .id("problem-counts")
                    .gap_1()
                    .cursor_pointer()
                    .child(Icon::new(IconName::XCircle).size(IconSize::XSmall).color(
                        if errors > 0 {
                            Color::Error
                        } else {
                            Color::Muted
                        },
                    ))
                    .child(Label::new(errors.to_string()).size(LabelSize::Small))
                    .child(Icon::new(IconName::Warning).size(IconSize::XSmall).color(
                        if warnings > 0 {
                            Color::Warning
                        } else {
                            Color::Muted
                        },
                    ))
                    .child(Label::new(warnings.to_string()).size(LabelSize::Small))
                    .tooltip(Tooltip::text("Problems (Ctrl+Shift+M)"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.run_command("problems:show", window, cx)
                    }))
                    .into_any_element(),
            );
        }
        let Some(status) = status else {
            return out;
        };
        /// What clicking the server label does.
        enum Click {
            Nothing,
            Info(String),
            Install,
            ShowLog,
            Restart,
        }
        let (text, color, click, tip): (String, Color, Click, &str) = match status {
            ServerStatus::Starting(name) => (format!("{name}…"), Color::Muted, Click::Nothing, ""),
            ServerStatus::Ready { name, progress } => {
                (progress.unwrap_or(name), Color::Muted, Click::Nothing, "")
            }
            ServerStatus::Stopped { name, message } => (
                format!("{name} stopped"),
                Color::Error,
                Click::Restart,
                if message.is_empty() {
                    "Restart"
                } else {
                    "Click to restart"
                },
            ),
            ServerStatus::Missing { language, .. } => (
                format!("Install {language} server"),
                Color::Accent,
                Click::Install,
                "Install the language server for completions, problems and go to definition",
            ),
            ServerStatus::Installing { what } => (
                format!("Installing {what}…"),
                Color::Muted,
                Click::Info(format!("Installing {what}; it starts by itself when done")),
                "",
            ),
            ServerStatus::InstallFailed { language, error } => (
                format!("{language} server install failed"),
                Color::Error,
                Click::ShowLog,
                if error.is_empty() {
                    ""
                } else {
                    "Click for the install log"
                },
            ),
        };
        let mut label = div()
            .id("lsp-status")
            .max_w(px(280.))
            .overflow_hidden()
            .child(
                Label::new(text)
                    .size(LabelSize::Small)
                    .color(color)
                    .truncate(),
            );
        if !tip.is_empty() {
            label = label.tooltip(Tooltip::text(tip.to_string()));
        }
        match click {
            Click::Nothing => {}
            Click::Info(message) => {
                label = label
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.info(message.clone(), cx)));
            }
            Click::Install => {
                label = label
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.run_command("language-server:install", window, cx)
                    }));
            }
            Click::ShowLog => {
                label = label
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.run_command("language-server:show-install-log", window, cx)
                    }));
            }
            Click::Restart => {
                label = label
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.run_command("language-server:restart", window, cx)
                    }));
            }
        }
        out.push(label.into_any_element());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_payloads_round_trip_paths_with_tabs_and_spaces() {
        let path = PathBuf::from("/a b/c\td.rs");
        let payload = location_payload(&path, Position::new(4, 7), PositionEncoding::Utf8);
        assert_eq!(
            parse_location_payload(&payload),
            Some((path, Position::new(4, 7), PositionEncoding::Utf8))
        );
        assert_eq!(parse_location_payload("nonsense"), None);
    }
}
