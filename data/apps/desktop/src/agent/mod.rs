//! The Agent pane: FerrumChat beside the editor and results, without a
//! project. Before each message it learns what the window shows — the
//! active tab's connection, its query, the last run's result or error and
//! the tables loaded in the tree — and its tools get that connection (see
//! [`tools`]).

mod tools;

use std::rc::Rc;

use ferrum_ui::prelude::*;
use ferrumagent_ui::core::host::AppHost;
use ferrumagent_ui::{AgentPane, ChatView};
use ferrumdata_domain::{ConnectionProfile, Endpoint};

use crate::app::FerrumData;
use crate::state::{AppState, ConnStatus, SchemaLoad};
pub use tools::ProfileSlot;

/// Characters of the tab's query in the context at most.
const QUERY_CHARS: usize = 4000;
/// Result rows and tables in the context at most.
const CONTEXT_ROWS: usize = 20;
const CONTEXT_TABLES: usize = 60;

const ABOUT: &str = "FerrumData is a database IDE for PostgreSQL, MySQL, SQLite, SurrealDB, \
                     MongoDB and Redis: connections, a query editor, results, table and key \
                     browsers.";

impl FerrumData {
    /// The top-right Agent toggle: shows the chat (made the first time) or
    /// hides it.
    pub fn toggle_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let slot = self.agent_profile.clone();
        let services = self.services.clone();
        let weak = cx.weak_entity();
        self.agent.toggle(window, cx, move |window, cx| {
            let host = AppHost {
                tools: tools::tools(&slot, &services),
                ..AppHost::new("FerrumData", ABOUT)
            };
            let context = Rc::new(move |cx: &mut App| {
                weak.update(cx, |app, _| {
                    app.agent_profile.set(app.state.active_profile().cloned());
                    context(&app.state)
                })
                .unwrap_or_default()
            });
            ChatView::without_project(host, AgentPane::options(Some(context)), window, cx)
        });
        cx.notify();
    }

    /// The Agent toggle, before the account.
    pub(crate) fn agent_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        ferrumagent_ui::agent_toggle("toggle-agent", self.agent.is_shown())
            .on_click(cx.listener(|this, _, window, cx| this.toggle_agent(window, cx)))
    }
}

fn describe(profile: &ConnectionProfile) -> String {
    let target = match &profile.endpoint {
        Endpoint::Tcp { host, port } => format!("{host}:{port}"),
        Endpoint::File { path } => path.clone(),
        // May hold more than a host: not repeated.
        Endpoint::Uri { .. } => "a connection string".into(),
    };
    let mut text = format!(
        "{} ({}, {target}",
        profile.display_name,
        profile.engine.label()
    );
    if let Some(db) = &profile.database {
        text.push_str(&format!(", database {db}"));
    }
    text.push_str(&format!(", {}", profile.env.label()));
    if profile.read_only {
        text.push_str(", read-only");
    }
    text.push(')');
    text
}

/// What the window shows, for the chat.
pub fn context(state: &AppState) -> String {
    let mut out = String::new();
    let profile = state.active_profile();
    match profile {
        Some(p) => {
            out.push_str(&format!("Active connection: {}", describe(p)));
            match state.conn_status.get(&p.id) {
                Some(ConnStatus::Ready(server)) => {
                    out.push_str(&format!(" — connected ({server})"))
                }
                Some(ConnStatus::Failed(e)) => out.push_str(&format!(" — failed: {e}")),
                Some(ConnStatus::Connecting) => out.push_str(" — connecting"),
                None => out.push_str(" — not connected yet"),
            }
            out.push('\n');
        }
        None => out.push_str("Active connection: none (the tab has no connection)\n"),
    }
    let others: Vec<String> = state
        .profiles
        .iter()
        .filter(|p| Some(p.id) != profile.map(|p| p.id))
        .map(|p| format!("{} ({})", p.display_name, p.engine.label()))
        .collect();
    if !others.is_empty() {
        out.push_str(&format!("Other connections: {}\n", others.join(", ")));
    }
    if let Some(tab) = state.active_tab() {
        if let Some(table) = &tab.table {
            out.push_str(&format!(
                "Tab \"{}\" browses the table {}\n",
                tab.title, table.name
            ));
        } else if let Some(key) = &tab.key {
            out.push_str(&format!("Tab \"{}\" shows the key {key}\n", tab.title));
        } else {
            out.push_str(&format!("Tab \"{}\" (query editor)\n", tab.title));
        }
        let query = tab.sql.trim();
        if !query.is_empty() {
            let short: String = query.chars().take(QUERY_CHARS).collect();
            out.push_str(&format!("Editor text:\n```\n{short}\n```\n"));
        }
        if let Some(run) = state.runs.get(&tab.id) {
            out.push_str(&format!(
                "Last run: `{}` — {}\n",
                run.sql,
                run.status_label()
            ));
            if let Some(error) = &run.error {
                out.push_str(&format!("Error: {}\n", error.message));
            }
            let columns: Vec<&str> = run
                .buffer
                .columns()
                .iter()
                .map(|c| c.name.as_str())
                .collect();
            if !columns.is_empty() {
                out.push_str(&format!("Result (first rows):\n{}\n", columns.join(" | ")));
                for (_, row) in run.buffer.rows(0..run.buffer.len().min(CONTEXT_ROWS)) {
                    let cells: Vec<String> = row.iter().map(|v| v.display()).collect();
                    out.push_str(&format!("{}\n", cells.join(" | ")));
                }
            }
        }
    }
    if let Some(p) = profile
        && let Some(SchemaLoad::Loaded(tables)) = state.schemas.get(&p.id)
    {
        let names: Vec<String> = tables
            .iter()
            .take(CONTEXT_TABLES)
            .map(|t| t.table.name.clone())
            .collect();
        out.push_str(&format!(
            "Tables ({}): {}\n",
            tables.len(),
            names.join(", ")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::EditorTab;
    use ferrumdata_domain::{DatabaseEngine, EnvironmentTag};

    #[test]
    fn the_context_names_the_connection_the_query_and_the_tables() {
        let mut state = AppState::default();
        let mut shop = ConnectionProfile::new(DatabaseEngine::Postgres, "shop");
        shop.env = EnvironmentTag::Production;
        shop.read_only = true;
        let other = ConnectionProfile::new(DatabaseEngine::Sqlite, "notes");
        let mut tab = EditorTab::new(1, Some(shop.id));
        tab.sql = "select * from orders".into();
        state.active_tab = Some(tab.id);
        state.tabs.push(tab);
        state.conn_status.insert(
            shop.id,
            ConnStatus::Failed("password authentication failed".into()),
        );
        state.profiles = vec![shop, other];
        let text = context(&state);
        assert!(
            text.contains("Active connection: shop (PostgreSQL"),
            "{text}"
        );
        assert!(text.contains("Production, read-only)"), "{text}");
        assert!(
            text.contains("failed: password authentication failed"),
            "{text}"
        );
        assert!(text.contains("Other connections: notes"), "{text}");
        assert!(text.contains("```\nselect * from orders\n```"), "{text}");

        let empty = context(&AppState::default());
        assert_eq!(
            empty,
            "Active connection: none (the tab has no connection)\n"
        );
    }
}
