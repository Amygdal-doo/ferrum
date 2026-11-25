//! Search Everywhere (Shift twice), Go to (⌘P) and Find in files (⌘⇧F) in
//! the shared search modal: connections, open tabs and loaded tables by
//! name, loaded columns as symbols, and text in the open tabs' SQL and the
//! loaded query history. Nothing from the vault (passwords) is listed.

use std::sync::Arc;

use ferrum_ui::components::IconName;
use ferrum_ui::prelude::*;
use ferrum_ui::search::{
    SearchCategory, SearchEvent, SearchItem, SearchModal, SearchMode, TextHit, TextMatcher,
    TextSearcher,
};
use ferrumdata_domain::{EditorTabId, ProfileId, TableKind, TableRef};

use crate::app::FerrumData;
use crate::state::{AppState, DetailsLoad, SchemaLoad};

const CONNECTION: &str = "conn:";
const TAB: &str = "tab:";
const TABLE: &str = "table:";
const HISTORY: &str = "history:";
/// Separates the parts of a table id (`table:<profile>\t<schema>\t<name>`).
const SEP: char = '\t';

fn table_id(profile: ProfileId, table: &TableRef) -> String {
    format!(
        "{TABLE}{profile}{SEP}{}{SEP}{}",
        table.schema.as_deref().unwrap_or_default(),
        table.name
    )
}

fn parse_table_id(id: &str) -> Option<(ProfileId, TableRef)> {
    let mut parts = id.strip_prefix(TABLE)?.split(SEP);
    let profile = parts.next()?.parse().ok()?;
    let schema = parts.next()?;
    let name = parts.next()?;
    Some((
        profile,
        TableRef {
            schema: (!schema.is_empty()).then(|| schema.to_owned()),
            name: name.to_owned(),
        },
    ))
}

fn qualified(table: &TableRef) -> String {
    match &table.schema {
        Some(schema) => format!("{schema}.{}", table.name),
        None => table.name.clone(),
    }
}

/// Connections, open tabs and loaded tables (Items) and loaded columns
/// (Symbols).
pub fn items(state: &AppState) -> Vec<SearchItem> {
    let mut items = Vec::new();
    for p in &state.profiles {
        items.push(
            SearchItem::new(
                SearchCategory::Items,
                p.display_name.clone(),
                format!("{CONNECTION}{}", p.id),
            )
            .detail(format!("Connection · {}", p.engine.label()))
            .icon(IconName::DatabaseZap),
        );
    }
    for t in &state.tabs {
        items.push(
            SearchItem::new(
                SearchCategory::Items,
                t.title.clone(),
                format!("{TAB}{}", t.id),
            )
            .detail("Open tab")
            .icon(IconName::Code),
        );
    }
    for p in &state.profiles {
        let Some(SchemaLoad::Loaded(tables)) = state.schemas.get(&p.id) else {
            continue;
        };
        for info in tables {
            let kind = if info.kind == TableKind::Table {
                "Table".to_owned()
            } else {
                let label = info.kind.label();
                format!("{}{}", label[..1].to_uppercase(), &label[1..])
            };
            items.push(
                SearchItem::new(
                    SearchCategory::Items,
                    qualified(&info.table),
                    table_id(p.id, &info.table),
                )
                .detail(format!("{kind} · {}", p.display_name))
                .icon(IconName::Hash),
            );
        }
    }
    for ((profile, table), details) in &state.table_details {
        let DetailsLoad::Loaded(details) = details else {
            continue;
        };
        let conn = state
            .profile(*profile)
            .map(|p| p.display_name.as_str())
            .unwrap_or_default();
        for column in &details.columns {
            items.push(
                SearchItem::new(
                    SearchCategory::Symbols,
                    format!("{}.{}", table.name, column.name),
                    table_id(*profile, table),
                )
                .detail(format!("Column · {conn}")),
            );
        }
    }
    items
}

/// Text in the open tabs' SQL and the history entries already loaded.
pub fn searcher(state: &AppState) -> TextSearcher {
    let tabs: Vec<(EditorTabId, String, String)> = state
        .tabs
        .iter()
        .filter(|t| t.key.is_none())
        .map(|t| (t.id, t.title.clone(), t.sql.clone()))
        .collect();
    let history: Vec<(i64, String, String)> = state
        .history
        .entries
        .iter()
        .map(|e| {
            let conn = state
                .profile(e.profile_id)
                .map_or("History", |p| p.display_name.as_str());
            (e.id, format!("{conn} history"), e.sql.clone())
        })
        .collect();
    let snapshot = Arc::new((tabs, history));
    Arc::new(move |query, cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let (tabs, history) = snapshot.as_ref();
        let mut hits = Vec::new();
        for (id, title, sql) in tabs {
            if matcher.allows(title)
                && !matcher.find_lines(sql, &format!("{TAB}{id}"), title, limit, &mut hits)
            {
                return Ok(hits);
            }
        }
        for (id, label, sql) in history {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            if matcher.allows(label)
                && !matcher.find_lines(sql, &format!("{HISTORY}{id}"), label, limit, &mut hits)
            {
                break;
            }
        }
        Ok(hits)
    })
}

impl FerrumData {
    /// Opens the search modal, or closes it when it is open in that mode.
    pub fn toggle_search(&mut self, mode: SearchMode, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = self.search.take()
            && search.read(cx).mode() == mode
        {
            self.search_subscription = None;
            window.focus(&self.focus_handle, cx);
            cx.notify();
            return;
        }
        let list = if mode == SearchMode::FindInFiles {
            Vec::new()
        } else {
            items(&self.state)
        };
        let searcher = (mode != SearchMode::GoTo).then(|| searcher(&self.state));
        let search = cx.new(|cx| SearchModal::new(mode, "Tables", list, searcher, window, cx));
        self.search_subscription = Some(cx.subscribe_in(&search, window, Self::on_search_event));
        self.search = Some(search);
        cx.notify();
    }

    /// ⌘F: the sidebar's filter of connections and tables.
    pub fn find_in_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.sidebar_hidden = false;
        self.filter.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn on_search_event(
        &mut self,
        _: &Entity<SearchModal>,
        event: &SearchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search = None;
        self.search_subscription = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
        match event {
            SearchEvent::Open(item) => self.open_search_item(&item.id, cx),
            SearchEvent::OpenHit(hit) => self.open_search_hit(hit, cx),
            SearchEvent::Dismissed => {}
        }
    }

    fn open_search_item(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(profile) = id
            .strip_prefix(CONNECTION)
            .and_then(|p| p.parse::<ProfileId>().ok())
        {
            self.state.sidebar_hidden = false;
            if !self.state.expanded.contains(&profile) {
                self.toggle_tables(profile, cx);
            }
            self.state.bind_active_tab(Some(profile));
        } else if let Some(tab) = id.strip_prefix(TAB) {
            self.activate_tab(tab, cx);
        } else if let Some((profile, table)) = parse_table_id(id) {
            let info = match self.state.schemas.get(&profile) {
                Some(SchemaLoad::Loaded(tables)) => {
                    tables.iter().find(|t| t.table == table).cloned()
                }
                _ => None,
            };
            if let Some(info) = info {
                self.open_table(profile, info, cx);
            }
        }
        cx.notify();
    }

    fn open_search_hit(&mut self, hit: &TextHit, cx: &mut Context<Self>) {
        if let Some(tab) = hit.source.strip_prefix(TAB) {
            self.activate_tab(tab, cx);
        } else if let Some(id) = hit
            .source
            .strip_prefix(HISTORY)
            .and_then(|id| id.parse::<i64>().ok())
            && let Some(entry) = self
                .state
                .history
                .entries
                .iter()
                .find(|e| e.id == id)
                .cloned()
        {
            self.open_history_entry(&entry, false, cx);
        }
    }

    fn activate_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(tab) = self.state.tabs.iter().find(|t| t.id.to_string() == id) {
            self.state.active_tab = Some(tab.id);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::EditorTab;
    use ferrumdata_domain::TableInfo;
    use std::sync::atomic::AtomicBool;

    fn table(schema: Option<&str>, name: &str, kind: TableKind) -> TableInfo {
        TableInfo {
            table: TableRef {
                schema: schema.map(str::to_owned),
                name: name.into(),
            },
            kind,
            primary_key: Vec::new(),
            estimated_rows: None,
        }
    }

    #[test]
    fn table_ids_round_trip() {
        let profile = ProfileId::new();
        for t in [
            TableRef {
                schema: Some("public".into()),
                name: "users".into(),
            },
            TableRef {
                schema: None,
                name: "notes".into(),
            },
        ] {
            assert_eq!(parse_table_id(&table_id(profile, &t)), Some((profile, t)));
        }
        assert_eq!(parse_table_id("conn:x"), None);
    }

    #[test]
    fn lists_connections_tabs_tables_and_columns() {
        let conn = ferrumdata_domain::ConnectionProfile::new_postgres("prod");
        let profile = conn.id;
        let mut state = AppState::default();
        state.profiles.push(conn);
        state.tabs.push(EditorTab::new(1, None));
        let users = table(Some("public"), "users", TableKind::Table);
        state.schemas.insert(
            profile,
            SchemaLoad::Loaded(vec![
                users.clone(),
                table(Some("public"), "active_users", TableKind::View),
            ]),
        );
        let items = items(&state);
        let find = |title: &str| items.iter().find(|i| i.title.as_ref() == title);
        assert!(find("prod").is_some_and(|i| i.id.starts_with(CONNECTION)));
        assert!(find("SQL 1").is_some_and(|i| i.id.starts_with(TAB)));
        assert_eq!(
            find("public.users").map(|i| i.detail.as_ref()),
            Some("Table · prod")
        );
        assert_eq!(
            find("public.active_users").map(|i| i.detail.as_ref()),
            Some("View · prod")
        );
        assert!(items.iter().all(|i| i.category == SearchCategory::Items));
    }

    #[test]
    fn searches_tab_sql() {
        let mut state = AppState::default();
        let mut tab = EditorTab::new(1, None);
        tab.sql = "select *\nfrom users\nwhere id = 1".into();
        state.tabs.push(tab);
        let search = searcher(&state);
        let query = ferrum_ui::search::TextQuery {
            text: "users".into(),
            ..Default::default()
        };
        let hits = search(&query, &AtomicBool::new(false), 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 1);
        assert_eq!(hits[0].source_label.as_ref(), "SQL 1");
    }
}
