//! The agent's database tools, on the active tab's connection: its schema
//! (tables, or one table's columns and indexes) and a read-only query run
//! at once; any other statement goes through `data_write_query`, which the
//! chat asks the user to approve first.
//!
//! Telling reads from writes uses the same check as the production
//! confirmation (`safety::may_mutate_in`): a keyword guard, not a security
//! boundary. A profile marked read-only is still enforced by its driver.
//! Queries run like the editor's and appear in the query history.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ferrumagent_ui::core::host::HostTool;
use ferrumagent_ui::core::provider::ToolSpec;
use ferrumagent_ui::core::tools::{Access, ToolDef, ToolOutput};
use ferrumdata_application::{AppServices, safety};
use ferrumdata_domain::{
    ConnectionProfile, ExecutionId, QueryEvent, QueryLimits, QueryRequest, TableRef,
};
use serde_json::{Value, json};

/// Rows a query returns to the model at most.
const MAX_ROWS: usize = 500;
/// Rows when the model does not say.
const ROWS: usize = 100;
/// Tables listed at most.
const MAX_TABLES: usize = 300;
/// Characters of output at most.
const OUTPUT_LIMIT: usize = 30 * 1024;
const QUERY_TIMEOUT: Duration = Duration::from_secs(60);

/// The connection the tools use: the active tab's, set by the window
/// before each message.
#[derive(Clone, Default)]
pub struct ProfileSlot(Arc<Mutex<Option<ConnectionProfile>>>);

impl ProfileSlot {
    pub fn set(&self, profile: Option<ConnectionProfile>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = profile;
    }

    fn get(&self) -> Result<ConnectionProfile, String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| "The active tab has no connection. Choose one in FerrumData.".into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Schema,
    Read,
    Write,
}

pub struct DataTool {
    kind: Kind,
    profile: ProfileSlot,
    services: AppServices,
}

/// The tools, all on `profile`.
pub fn tools(profile: &ProfileSlot, services: &AppServices) -> Vec<Arc<dyn HostTool>> {
    [Kind::Schema, Kind::Read, Kind::Write]
        .into_iter()
        .map(|kind| {
            Arc::new(DataTool {
                kind,
                profile: profile.clone(),
                services: services.clone(),
            }) as Arc<dyn HostTool>
        })
        .collect()
}

#[async_trait::async_trait]
impl HostTool for DataTool {
    fn def(&self) -> ToolDef {
        let query = json!({"type": "string", "description": "One statement in the connection's query language"});
        let rows = json!({"type": "integer", "minimum": 1, "maximum": MAX_ROWS});
        let (name, description, schema, access) = match self.kind {
            Kind::Schema => (
                "data_schema",
                "The connected database's tables and views; with `table` (`name` or `schema.name`), its columns and indexes.",
                json!({"type": "object", "properties": {"table": {"type": "string"}}}),
                Access::Read,
            ),
            Kind::Read => (
                "data_read_query",
                "Run a read-only query (SELECT, SHOW, EXPLAIN…) on the connected database and return its rows. Statements that may change data are refused: use data_write_query.",
                json!({"type": "object", "properties": {"query": query, "max_rows": rows}, "required": ["query"]}),
                Access::Read,
            ),
            Kind::Write => (
                "data_write_query",
                "Run a statement that changes data or schema (INSERT, UPDATE, DELETE, CREATE…). The user approves it first.",
                json!({"type": "object", "properties": {"query": query}, "required": ["query"]}),
                Access::Execute,
            ),
        };
        ToolDef {
            spec: ToolSpec {
                name: name.into(),
                description: description.into(),
                input_schema: schema,
            },
            access,
        }
    }

    fn title(&self, input: &Value) -> String {
        let query = input["query"].as_str().unwrap_or_default();
        let short: String = query.split_whitespace().collect::<Vec<_>>().join(" ");
        let short: String = short.chars().take(60).collect();
        match self.kind {
            Kind::Schema => match input["table"].as_str() {
                Some(table) => format!("Describe {table}"),
                None => "List tables".into(),
            },
            Kind::Read => format!("Query `{short}`"),
            Kind::Write => format!("Change data `{short}`"),
        }
    }

    async fn call(&self, input: Value) -> ToolOutput {
        let profile = match self.profile.get() {
            Ok(profile) => profile,
            Err(e) => return ToolOutput::error(e),
        };
        let services = self.services.clone();
        let kind = self.kind;
        let work = async move {
            match kind {
                Kind::Schema => schema(&services, &profile, input["table"].as_str()).await,
                Kind::Read | Kind::Write => {
                    let query = input["query"].as_str().unwrap_or_default().trim();
                    if query.is_empty() {
                        return Err("The query is empty.".into());
                    }
                    if kind == Kind::Read && safety::may_mutate_in(profile.engine, query) {
                        return Err(
                            "This statement may change data: use data_write_query, which the \
                             user approves first."
                                .into(),
                        );
                    }
                    let rows = input["max_rows"]
                        .as_u64()
                        .map_or(ROWS, |n| usize::try_from(n).unwrap_or(MAX_ROWS))
                        .clamp(1, MAX_ROWS);
                    run(&services, &profile, query, rows).await
                }
            }
        };
        match self.services.spawn(work).await {
            Ok(Ok(text)) => ToolOutput::ok(limit(text)),
            Ok(Err(e)) => ToolOutput::error(e),
            Err(e) => ToolOutput::error(format!("background task failed: {e}")),
        }
    }
}

fn limit(text: String) -> String {
    if text.len() <= OUTPUT_LIMIT {
        return text;
    }
    let mut end = OUTPUT_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…(output truncated)", &text[..end])
}

async fn schema(
    services: &AppServices,
    profile: &ConnectionProfile,
    table: Option<&str>,
) -> Result<String, String> {
    let connections = &services.connections;
    let Some(table) = table.map(str::trim).filter(|t| !t.is_empty()) else {
        let tables = connections
            .list_tables(profile.id)
            .await
            .map_err(|e| e.message)?;
        let mut out = format!(
            "{} ({}): {} tables and views\n",
            profile.display_name,
            profile.engine.label(),
            tables.len()
        );
        for t in tables.iter().take(MAX_TABLES) {
            out.push_str(&format!("- {} ({:?})", name_of(&t.table), t.kind));
            if !t.primary_key.is_empty() {
                out.push_str(&format!(", primary key {}", t.primary_key.join(", ")));
            }
            if let Some(rows) = t.estimated_rows {
                out.push_str(&format!(", ~{rows} rows"));
            }
            out.push('\n');
        }
        return Ok(out);
    };
    let table = match table.split_once('.') {
        Some((schema, name)) => TableRef {
            schema: Some(schema.to_string()),
            name: name.to_string(),
        },
        None => TableRef {
            schema: None,
            name: table.to_string(),
        },
    };
    let details = connections
        .describe_table(profile.id, table.clone())
        .await
        .map_err(|e| e.message)?;
    let mut out = format!("{}:\n", name_of(&table));
    for c in &details.columns {
        out.push_str(&format!("- {}: {}\n", c.name, c.summary()));
    }
    if !details.indexes.is_empty() {
        out.push_str("Indexes:\n");
        for i in &details.indexes {
            out.push_str(&format!("- {} ({})\n", i.name, i.columns.join(", ")));
        }
    }
    Ok(out)
}

fn name_of(table: &TableRef) -> String {
    match &table.schema {
        Some(schema) => format!("{schema}.{}", table.name),
        None => table.name.clone(),
    }
}

/// Runs `query`, keeping `max_rows` rows: a header line and one line per
/// row, values separated by ` | `.
async fn run(
    services: &AppServices,
    profile: &ConnectionProfile,
    query: &str,
    max_rows: usize,
) -> Result<String, String> {
    let request = QueryRequest {
        execution_id: ExecutionId::new(),
        profile_id: profile.id,
        editor_id: None,
        sql: query.to_string(),
        limits: QueryLimits {
            max_rows,
            timeout: Some(QUERY_TIMEOUT),
            ..QueryLimits::default()
        },
    };
    let mut events = services
        .queries
        .execute(request)
        .await
        .map_err(|e| e.message)?;
    let mut out = String::new();
    let mut rows = 0usize;
    let mut notes = Vec::new();
    while let Some(envelope) = events.recv().await {
        match envelope.event {
            QueryEvent::Started => {}
            QueryEvent::Metadata(columns) => {
                let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
                out.push_str(&names.join(" | "));
                out.push('\n');
            }
            QueryEvent::Rows(batch) => {
                for row in batch {
                    rows += 1;
                    let cells: Vec<String> = row.iter().map(|v| v.display()).collect();
                    out.push_str(&cells.join(" | "));
                    out.push('\n');
                }
            }
            QueryEvent::Message(message) => notes.push(message),
            QueryEvent::Completed(outcome) => {
                let mut summary = match outcome.rows_affected {
                    Some(n) if rows == 0 => format!("{n} rows affected"),
                    _ => format!("{rows} rows"),
                };
                if outcome.truncated {
                    summary.push_str(&format!(" (stopped at {max_rows})"));
                }
                out.push_str(&format!("({summary}, {} ms)", outcome.elapsed.as_millis()));
                for note in notes {
                    out.push_str(&format!("\n{note}"));
                }
                return Ok(out);
            }
            QueryEvent::Cancelled { .. } => return Err("The query was cancelled.".into()),
            QueryEvent::TimedOut { elapsed } => {
                return Err(format!(
                    "The query timed out after {} s.",
                    elapsed.as_secs()
                ));
            }
            QueryEvent::Failed(e) => return Err(e.message),
        }
    }
    Err("The query ended without a result.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_application::PasswordChange;
    use ferrumdata_domain::{DatabaseEngine, Endpoint};

    struct Fixture {
        _dir: tempfile::TempDir,
        tools: Vec<Arc<dyn HostTool>>,
    }

    /// A SQLite database in a temporary folder, as the active connection.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let services = AppServices::start(&dir.path().join("state")).expect("services");
        let file = dir.path().join("shop.db");
        // The driver opens existing files only; an empty one is a new database.
        std::fs::write(&file, b"").expect("db file");
        let mut profile = ConnectionProfile::new(DatabaseEngine::Sqlite, "shop");
        profile.endpoint = Endpoint::File {
            path: file.to_string_lossy().to_string(),
        };
        let connections = services.connections.clone();
        let profile = services
            .spawn(async move {
                connections
                    .save_profile(profile, PasswordChange::Keep)
                    .await
            })
            .join()
            .expect("saved");
        let slot = ProfileSlot::default();
        slot.set(Some(profile));
        let tools = tools(&slot, &services);
        Fixture { _dir: dir, tools }
    }

    trait Join<T> {
        fn join(self) -> T;
    }

    impl<T> Join<T> for tokio::task::JoinHandle<T> {
        fn join(self) -> T {
            futures::executor::block_on(self).expect("task")
        }
    }

    impl Fixture {
        fn run(&self, name: &str, input: Value) -> ToolOutput {
            let tool = self
                .tools
                .iter()
                .find(|t| t.def().spec.name == name)
                .expect("tool");
            futures::executor::block_on(tool.call(input))
        }
    }

    #[test]
    fn schema_and_reads_run_at_once_and_writes_ask() {
        let f = fixture();
        let access = |name: &str| {
            f.tools
                .iter()
                .find(|t| t.def().spec.name == name)
                .map(|t| t.def().access)
        };
        assert_eq!(access("data_schema"), Some(Access::Read));
        assert_eq!(access("data_read_query"), Some(Access::Read));
        assert_eq!(access("data_write_query"), Some(Access::Execute));
    }

    #[test]
    fn writes_then_reads_and_describes_a_sqlite_database() {
        let f = fixture();
        let out = f.run(
            "data_write_query",
            json!({"query": "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"}),
        );
        assert!(!out.is_error, "{}", out.text);
        let out = f.run(
            "data_write_query",
            json!({"query": "INSERT INTO items (name) VALUES ('tea'), ('cake')"}),
        );
        assert!(!out.is_error, "{}", out.text);

        let out = f.run(
            "data_read_query",
            json!({"query": "SELECT id, name FROM items ORDER BY id", "max_rows": 1}),
        );
        assert!(!out.is_error, "{}", out.text);
        assert!(out.text.starts_with("id | name\n1 | tea\n"), "{}", out.text);
        assert!(out.text.contains("(stopped at 1)"), "{}", out.text);

        let out = f.run("data_schema", json!({}));
        assert!(out.text.contains("- items"), "{}", out.text);
        let out = f.run("data_schema", json!({"table": "items"}));
        assert!(out.text.contains("- name: "), "{}", out.text);
    }

    #[test]
    fn the_read_tool_refuses_statements_that_change_data() {
        let f = fixture();
        f.run(
            "data_write_query",
            json!({"query": "CREATE TABLE t (x INTEGER)"}),
        );
        let out = f.run("data_read_query", json!({"query": "DROP TABLE t"}));
        assert!(out.is_error);
        assert!(out.text.contains("data_write_query"));
        let out = f.run("data_schema", json!({}));
        assert!(
            out.text.contains("- t"),
            "the table is still there: {}",
            out.text
        );
    }

    #[test]
    fn without_a_connection_the_tools_say_so() {
        let dir = tempfile::tempdir().expect("tempdir");
        let services = AppServices::start(dir.path()).expect("services");
        let tools = tools(&ProfileSlot::default(), &services);
        let out = futures::executor::block_on(tools[0].call(json!({})));
        assert!(out.is_error && out.text.contains("no connection"));
    }
}
