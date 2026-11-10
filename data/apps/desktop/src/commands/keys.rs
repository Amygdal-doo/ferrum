//! Key explorer and key tabs for key-value (Redis) connections: SCAN pages,
//! opening a key's value, and the alpha edits (string value, TTL, delete).

use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::{ConnStatus, EditorTab, KeyViewLoad, TabRun};
use ferrum_ui::prelude::*;
use ferrumdata_application::ConnectionService;
use ferrumdata_domain::{
    DbError, EditorTabId, ExecutionId, KeyKind, KeyValueView, ProfileId, QueryEvent, QueryOutcome,
    keyvalue::format_bytes,
};
use std::time::{Duration, Instant};

/// Keys requested per `SCAN` page ("Load more" fetches the next one).
pub const KEY_PAGE: usize = 200;

/// Text inputs of one key tab: the string value editor and the TTL box.
pub struct KeyInputs {
    pub value: Entity<TextInput>,
    pub ttl: Entity<TextInput>,
}

/// TTL typed by the user: seconds, or a number with `s`, `m`, `h` or `d`.
#[must_use]
pub fn parse_ttl(text: &str) -> Option<Duration> {
    let text = text.trim().to_ascii_lowercase();
    let (number, unit) = match text.char_indices().last()? {
        (i, c @ ('s' | 'm' | 'h' | 'd')) => (&text[..i], c),
        _ => (text.as_str(), 's'),
    };
    let n: u64 = number.trim().parse().ok().filter(|n| *n > 0)?;
    let secs = match unit {
        'm' => n.checked_mul(60)?,
        'h' => n.checked_mul(3_600)?,
        'd' => n.checked_mul(86_400)?,
        _ => n,
    };
    Some(Duration::from_secs(secs))
}

/// A key's value as a finished grid run, so the results grid (sort, copy,
/// export) shows it like a query result.
fn value_run(
    profile_id: ProfileId,
    view: &KeyValueView,
    max_rows: usize,
    elapsed: Duration,
) -> TabRun {
    let rows = view.rows.len();
    let mut run = TabRun::new(
        ExecutionId::new(),
        profile_id,
        format!("key {}", view.key),
        max_rows.max(rows),
    );
    run.apply(QueryEvent::Started);
    run.apply(QueryEvent::Metadata(view.columns.clone()));
    run.apply(QueryEvent::Rows(view.rows.clone()));
    run.apply(QueryEvent::Completed(QueryOutcome {
        rows_returned: rows,
        rows_affected: None,
        truncated: false,
        elapsed,
    }));
    if view.truncated {
        run.note = Some(match (&view.kind, view.length) {
            (KeyKind::String, Some(len)) => format!(
                "Showing the first {} of {}; editing is disabled for truncated values.",
                format_bytes(view.text.as_ref().map_or(0, |t| t.len() as u64)),
                format_bytes(len)
            ),
            (_, Some(len)) => format!("Showing the first {rows} of {len} elements (row limit)."),
            (_, None) => format!("Showing the first {rows} elements (row limit)."),
        });
    }
    run
}

impl FerrumData {
    /// The pattern box above a connection's key tree, created on first use.
    pub fn key_pattern_input(
        &mut self,
        id: ProfileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        if let Some((input, _)) = self.key_patterns.get(&id) {
            return input.clone();
        }
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .monospace()
                .placeholder("Pattern, e.g. user:*  (Enter scans)")
        });
        let subscription = cx.subscribe_in(&input, window, move |this, _, event, _, cx| {
            if matches!(event, TextInputEvent::Submit) {
                this.scan_keys(id, true, cx);
            }
        });
        self.key_patterns.insert(id, (input.clone(), subscription));
        input
    }

    /// Starts a new scan with the pattern box text (`restart`), or loads the
    /// next page of the current one. Never uses `KEYS`.
    pub fn scan_keys(&mut self, id: ProfileId, restart: bool, cx: &mut Context<Self>) {
        let pattern = self
            .key_patterns
            .get(&id)
            .map(|(input, _)| input.read(cx).text().trim().to_owned())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "*".to_owned());
        let browser = self.state.key_browsers.entry(id).or_default();
        if restart {
            browser.restart(pattern);
        } else if browser.loading || browser.complete {
            return;
        } else {
            browser.loading = true;
        }
        let (pattern, cursor, generation) =
            (browser.pattern.clone(), browser.cursor, browser.generation);
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                let total = if restart {
                    svc.connections.key_count(id).await.ok()
                } else {
                    None
                };
                let page = svc
                    .connections
                    .scan_keys(id, pattern, cursor, KEY_PAGE)
                    .await?;
                Ok::<_, DbError>((page, total))
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let label = this
                    .state
                    .profile(id)
                    .map_or("Connected", |p| p.engine.label());
                let Some(browser) = this.state.key_browsers.get_mut(&id) else {
                    return;
                };
                // A newer scan replaced this one.
                if browser.generation != generation {
                    return;
                }
                match result {
                    Ok(Ok((page, total))) => {
                        browser.append(page);
                        if total.is_some() {
                            browser.total = total;
                        }
                        if !matches!(this.state.conn_status.get(&id), Some(ConnStatus::Ready(_))) {
                            this.state
                                .conn_status
                                .insert(id, ConnStatus::Ready(label.into()));
                        }
                    }
                    Ok(Err(e)) => {
                        browser.loading = false;
                        browser.error = Some(e.message);
                    }
                    Err(e) => {
                        browser.loading = false;
                        browser.error = Some(e);
                    }
                }
                cx.notify();
            }));
        })
        .detach();
    }

    pub fn toggle_key_folder(&mut self, id: ProfileId, prefix: String, cx: &mut Context<Self>) {
        let browser = self.state.key_browsers.entry(id).or_default();
        if !browser.expanded.remove(&prefix) {
            browser.expanded.insert(prefix);
        }
        cx.notify();
    }

    /// Focuses the tab showing this key (reloading it), or opens a new one.
    pub fn open_key(&mut self, profile_id: ProfileId, key: String, cx: &mut Context<Self>) {
        let existing = self
            .state
            .tabs
            .iter()
            .find(|t| t.profile_id == Some(profile_id) && t.key.as_deref() == Some(key.as_str()))
            .map(|t| t.id);
        let tab_id = if let Some(id) = existing {
            id
        } else {
            let mut tab = EditorTab::new(0, Some(profile_id));
            tab.title.clone_from(&key);
            tab.key = Some(key);
            let id = tab.id;
            self.state.tabs.push(tab);
            id
        };
        self.state.active_tab = Some(tab_id);
        self.load_key_view(tab_id, cx);
    }

    /// (profile, key) of a key tab.
    fn key_tab(&self, tab_id: EditorTabId) -> Option<(ProfileId, String)> {
        let tab = self.state.tabs.iter().find(|t| t.id == tab_id)?;
        Some((tab.profile_id?, tab.key.clone()?))
    }

    /// (Re)loads the value shown in a key tab.
    pub fn load_key_view(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let Some((profile_id, key)) = self.key_tab(tab_id) else {
            return;
        };
        let max_rows = self.state.settings.query_limits.max_rows;
        self.state.key_views.insert(tab_id, KeyViewLoad::Loading);
        self.state.runs.remove(&tab_id);
        cx.notify();
        let services = self.services.clone();
        let started = Instant::now();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.connections.inspect_key(profile_id, key, max_rows).await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                // The tab was closed meanwhile.
                if !this.state.key_views.contains_key(&tab_id) {
                    return;
                }
                let load = match result {
                    Ok(Ok(view)) => {
                        if let Some(b) = this.state.key_browsers.get_mut(&profile_id) {
                            b.update_from(&view);
                        }
                        let run = value_run(profile_id, &view, max_rows, started.elapsed());
                        this.state.runs.insert(tab_id, run);
                        if let Some(inputs) = this.key_inputs.get(&tab_id) {
                            let text = view.text.clone().unwrap_or_default();
                            inputs.value.update(cx, |i, cx| i.set_text(text, cx));
                        }
                        KeyViewLoad::Loaded(Box::new(view))
                    }
                    Ok(Err(e)) => KeyViewLoad::Failed(e.message),
                    Err(e) => KeyViewLoad::Failed(e),
                };
                this.state.key_views.insert(tab_id, load);
                cx.notify();
            }));
        })
        .detach();
    }

    /// Runs one key edit in the background, then reloads the tab.
    fn key_edit<T, F>(
        &mut self,
        tab_id: EditorTabId,
        done: &'static str,
        cx: &mut Context<Self>,
        op: impl FnOnce(ConnectionService, ProfileId, String) -> F,
    ) where
        T: Send + 'static,
        F: Future<Output = Result<T, DbError>> + Send + 'static,
    {
        let Some((profile_id, key)) = self.key_tab(tab_id) else {
            return;
        };
        let future = op(self.services.connections.clone(), profile_id, key);
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let result = background(&services, future).await;
            log_dropped(this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(_)) => this.notify_user(done, false, cx),
                    Ok(Err(e)) => this.notify_user(e.message, true, cx),
                    Err(e) => this.notify_user(e, true, cx),
                }
                this.load_key_view(tab_id, cx);
            }));
        })
        .detach();
    }

    /// Saves the value editor's text as the key's new string value (TTL kept).
    pub fn save_key_value(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let Some(value) = self
            .key_inputs
            .get(&tab_id)
            .map(|i| i.value.read(cx).text().to_owned())
        else {
            return;
        };
        self.key_edit(tab_id, "Value saved.", cx, move |svc, id, key| async move {
            svc.set_key_string(id, key, value).await
        });
    }

    /// Applies the TTL typed in the tab's TTL box.
    pub fn set_key_ttl(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let text = self
            .key_inputs
            .get(&tab_id)
            .map(|i| i.ttl.read(cx).text().to_owned())
            .unwrap_or_default();
        let Some(ttl) = parse_ttl(&text) else {
            self.notify_user(
                "Enter a TTL such as 90, 15m, 2h or 1d (a positive number of seconds, minutes, hours or days).",
                true,
                cx,
            );
            return;
        };
        self.key_edit(tab_id, "TTL set.", cx, move |svc, id, key| async move {
            svc.set_key_ttl(id, key, Some(ttl)).await
        });
    }

    /// Removes the key's TTL (`PERSIST`).
    pub fn persist_key(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.key_edit(tab_id, "TTL removed.", cx, |svc, id, key| async move {
            svc.set_key_ttl(id, key, None).await
        });
    }

    /// Deletes a key (after the confirmation dialog) and closes its tabs.
    pub fn delete_key(&mut self, profile_id: ProfileId, key: String, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let k = key.clone();
            let result = background(&services, async move {
                svc.connections.delete_key(profile_id, k).await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| match result {
                Ok(Ok(existed)) => {
                    if let Some(b) = this.state.key_browsers.get_mut(&profile_id) {
                        b.remove(&key);
                    }
                    let tabs: Vec<_> = this
                        .state
                        .tabs
                        .iter()
                        .filter(|t| {
                            t.profile_id == Some(profile_id) && t.key.as_deref() == Some(&key)
                        })
                        .map(|t| t.id)
                        .collect();
                    for id in tabs {
                        this.close_tab(id, cx);
                    }
                    let msg = if existed {
                        "Key deleted."
                    } else {
                        "The key no longer existed."
                    };
                    this.notify_user(msg, false, cx);
                }
                Ok(Err(e)) => this.notify_user(e.message, true, cx),
                Err(e) => this.notify_user(e, true, cx),
            }));
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{CellValue, ColumnMeta, KeyTtl, LogicalType, QueryState};

    #[test]
    fn ttl_input_accepts_seconds_and_units() {
        assert_eq!(parse_ttl("90"), Some(Duration::from_secs(90)));
        assert_eq!(parse_ttl(" 15m "), Some(Duration::from_secs(900)));
        assert_eq!(parse_ttl("2H"), Some(Duration::from_secs(7_200)));
        assert_eq!(parse_ttl("1d"), Some(Duration::from_secs(86_400)));
        assert_eq!(parse_ttl("30s"), Some(Duration::from_secs(30)));
        for bad in ["", "0", "-5", "abc", "5x", "m"] {
            assert_eq!(parse_ttl(bad), None, "{bad}");
        }
    }

    #[test]
    fn values_become_completed_grid_runs_with_a_truncation_note() {
        let view = KeyValueView {
            key: "l".into(),
            kind: KeyKind::List,
            ttl: KeyTtl::Persistent,
            memory_bytes: None,
            length: Some(10),
            text: None,
            is_json: false,
            columns: vec![ColumnMeta {
                name: "index".into(),
                db_type: "integer".into(),
                logical_type: LogicalType::Integer,
                nullable: None,
                primary_key: true,
                editable: false,
            }],
            rows: vec![vec![CellValue::Signed(0)], vec![CellValue::Signed(1)]],
            truncated: true,
        };
        let run = value_run(ProfileId::new(), &view, 2, Duration::ZERO);
        assert_eq!(run.state, QueryState::Completed);
        assert_eq!(run.buffer.len(), 2);
        assert_eq!(
            run.note.as_deref(),
            Some("Showing the first 2 of 10 elements (row limit).")
        );
    }
}
