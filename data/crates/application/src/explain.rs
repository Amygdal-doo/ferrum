//! EXPLAIN for PostgreSQL (`FORMAT JSON`), MySQL (`FORMAT=TREE` /
//! `EXPLAIN ANALYZE`) and SQLite (`EXPLAIN QUERY PLAN`), parsed into one
//! tree, flattened for display, with the expensive nodes marked.

use crate::QueryService;
use ferrumdata_domain::{
    CellValue, DatabaseEngine, DbError, DbErrorKind, ExecutionId, ProfileId, QueryEvent,
    QueryLimits, QueryRequest,
};
use serde_json::Value;

/// Share of the whole plan's time (or cost) above which a node is "slow".
const SLOW_SHARE: f64 = 0.2;
/// Analysed nodes faster than this are never flagged.
const SLOW_MIN_MS: f64 = 1.0;

/// One plan node with the numbers the engine reported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanNode {
    pub label: String,
    /// Conditions, keys, index used…
    pub detail: Option<String>,
    /// Estimated total cost (planner units).
    pub cost: Option<f64>,
    /// Estimated rows per loop.
    pub rows: Option<f64>,
    pub actual_rows: Option<f64>,
    pub loops: Option<f64>,
    /// Inclusive time over all loops (ms), when analysed.
    pub total_ms: Option<f64>,
    pub children: Vec<PlanNode>,
}

/// A node as listed: its depth, time spent in the node itself (children
/// excluded) and whether it dominates the plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanRow {
    pub depth: usize,
    pub node: PlanNode,
    pub self_ms: Option<f64>,
    pub slow: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub rows: Vec<PlanRow>,
    pub analyzed: bool,
    pub planning_ms: Option<f64>,
    pub execution_ms: Option<f64>,
}

/// The EXPLAIN statement for `sql`; `Err` for engines without one.
pub fn explain_sql(engine: DatabaseEngine, sql: &str, analyze: bool) -> Result<String, String> {
    let sql = sql.trim().trim_end_matches(';');
    match (engine, analyze) {
        (DatabaseEngine::Postgres, false) => Ok(format!("EXPLAIN (FORMAT JSON) {sql}")),
        (DatabaseEngine::Postgres, true) => {
            Ok(format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}"))
        }
        (DatabaseEngine::MySql, false) => Ok(format!("EXPLAIN FORMAT=TREE {sql}")),
        (DatabaseEngine::MySql, true) => Ok(format!("EXPLAIN ANALYZE {sql}")),
        (DatabaseEngine::Sqlite, false) => Ok(format!("EXPLAIN QUERY PLAN {sql}")),
        (DatabaseEngine::Sqlite, true) => {
            Err("SQLite has no EXPLAIN ANALYZE: use Explain for the query plan".into())
        }
        _ => Err(format!("EXPLAIN is not available for {}", engine.label())),
    }
}

/// Runs EXPLAIN for `sql` (recorded in the query history like any run) and
/// parses the plan.
pub async fn run_explain(
    queries: &QueryService,
    engine: DatabaseEngine,
    profile_id: ProfileId,
    sql: &str,
    analyze: bool,
) -> Result<Plan, DbError> {
    let statement =
        explain_sql(engine, sql, analyze).map_err(|e| DbError::new(DbErrorKind::Unsupported, e))?;
    let request = QueryRequest {
        execution_id: ExecutionId::new(),
        profile_id,
        editor_id: None,
        sql: statement,
        limits: QueryLimits::default(),
    };
    let mut events = queries.execute(request).await?;
    let mut rows = Vec::new();
    while let Some(envelope) = events.recv().await {
        match envelope.event {
            QueryEvent::Rows(batch) => rows.extend(batch),
            QueryEvent::Failed(e) => return Err(e),
            QueryEvent::Cancelled { .. } => return Err(DbError::cancelled()),
            QueryEvent::TimedOut { .. } => {
                return Err(DbError::new(DbErrorKind::Timeout, "EXPLAIN timed out"));
            }
            _ => {}
        }
    }
    parse(engine, &rows, analyze).map_err(|e| DbError::new(DbErrorKind::Internal, e))
}

/// Parses the rows an EXPLAIN statement returned.
pub fn parse(
    engine: DatabaseEngine,
    rows: &[Vec<CellValue>],
    analyze: bool,
) -> Result<Plan, String> {
    let text = |cell: Option<&CellValue>| match cell {
        Some(CellValue::Text(s) | CellValue::Json(s)) => Some(s.clone()),
        // MySQL sends the tree as a binary-collated string.
        Some(CellValue::Binary(bytes)) => String::from_utf8(bytes.clone()).ok(),
        _ => None,
    };
    let mut plan = match engine {
        DatabaseEngine::Postgres => {
            let json = rows
                .first()
                .and_then(|r| text(r.first()))
                .ok_or("EXPLAIN returned no plan")?;
            parse_postgres(&json)?
        }
        DatabaseEngine::MySql => {
            let tree: Vec<String> = rows.iter().filter_map(|r| text(r.first())).collect();
            parse_mysql_tree(&tree.join("\n"))?
        }
        DatabaseEngine::Sqlite => {
            let entries: Vec<(i64, i64, String)> = rows
                .iter()
                .filter_map(|r| match (r.first(), r.get(1), r.get(3)) {
                    (Some(CellValue::Signed(id)), Some(CellValue::Signed(parent)), detail) => {
                        Some((*id, *parent, text(detail).unwrap_or_default()))
                    }
                    _ => None,
                })
                .collect();
            parse_sqlite(&entries)
        }
        _ => return Err(format!("EXPLAIN is not available for {}", engine.label())),
    };
    plan.analyzed = analyze;
    Ok(plan)
}

/// `[{"Plan": {...}, "Planning Time": …, "Execution Time": …}]`
pub fn parse_postgres(json: &str) -> Result<Plan, String> {
    let value: Value = serde_json::from_str(json).map_err(|e| format!("Unreadable plan: {e}"))?;
    let top = value.get(0).unwrap_or(&value);
    let root = top.get("Plan").ok_or("The plan has no root node")?;
    let mut plan = Plan {
        planning_ms: top.get("Planning Time").and_then(Value::as_f64),
        execution_ms: top.get("Execution Time").and_then(Value::as_f64),
        ..Plan::default()
    };
    flatten(&postgres_node(root), &mut plan.rows);
    mark_slow(&mut plan.rows);
    Ok(plan)
}

fn postgres_node(v: &Value) -> PlanNode {
    let s = |key: &str| v.get(key).and_then(Value::as_str);
    let f = |key: &str| v.get(key).and_then(Value::as_f64);
    let mut label = s("Node Type").unwrap_or("?").to_owned();
    if let Some(join) = s("Join Type").filter(|_| label.contains("Join") || label == "Nested Loop")
    {
        label = format!("{label} ({join})");
    }
    if let Some(index) = s("Index Name") {
        label.push_str(&format!(" using {index}"));
    }
    if let Some(relation) = s("Relation Name") {
        label.push_str(&format!(" on {relation}"));
        if let Some(alias) = s("Alias").filter(|a| *a != relation) {
            label.push_str(&format!(" {alias}"));
        }
    }
    let detail: Vec<String> = [
        "Index Cond",
        "Hash Cond",
        "Merge Cond",
        "Join Filter",
        "Filter",
        "Recheck Cond",
    ]
    .iter()
    .filter_map(|k| s(k).map(|c| format!("{k}: {c}")))
    .chain(["Sort Key", "Group Key"].iter().filter_map(|k| {
        let keys: Vec<&str> = v
            .get(*k)?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        Some(format!("{k}: {}", keys.join(", ")))
    }))
    .collect();
    let loops = f("Actual Loops");
    PlanNode {
        label,
        detail: (!detail.is_empty()).then(|| detail.join(" · ")),
        cost: f("Total Cost"),
        rows: f("Plan Rows"),
        actual_rows: f("Actual Rows"),
        loops,
        // Actual times are per loop.
        total_ms: f("Actual Total Time").map(|t| t * loops.unwrap_or(1.0)),
        children: v
            .get("Plans")
            .and_then(Value::as_array)
            .map(|plans| plans.iter().map(postgres_node).collect())
            .unwrap_or_default(),
    }
}

/// MySQL tree format: one `-> node  (cost=… rows=…) (actual time=a..b
/// rows=… loops=…)` per line, children indented by four spaces.
pub fn parse_mysql_tree(text: &str) -> Result<Plan, String> {
    let mut stack: Vec<(usize, PlanNode)> = Vec::new();
    let mut roots: Vec<PlanNode> = Vec::new();
    let pop_into = |stack: &mut Vec<(usize, PlanNode)>, roots: &mut Vec<PlanNode>| {
        if let Some((_, node)) = stack.pop() {
            match stack.last_mut() {
                Some((_, parent)) => parent.children.push(node),
                None => roots.push(node),
            }
        }
    };
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        let Some(body) = line.trim_start().strip_prefix("-> ") else {
            // A wrapped continuation of the previous node's text.
            if let Some((_, node)) = stack.last_mut()
                && !line.trim().is_empty()
            {
                node.label.push(' ');
                node.label.push_str(line.trim());
            }
            continue;
        };
        let depth = indent / 4;
        while stack.last().is_some_and(|(d, _)| *d >= depth) {
            pop_into(&mut stack, &mut roots);
        }
        stack.push((depth, mysql_node(body)));
    }
    while !stack.is_empty() {
        pop_into(&mut stack, &mut roots);
    }
    if roots.is_empty() {
        return Err("EXPLAIN returned no plan".into());
    }
    let mut plan = Plan::default();
    for root in &roots {
        flatten(root, &mut plan.rows);
    }
    mark_slow(&mut plan.rows);
    Ok(plan)
}

fn mysql_node(body: &str) -> PlanNode {
    let starts = [" (cost=", " (rows=", " (actual time=", " (never executed)"];
    let cut = starts
        .iter()
        .filter_map(|s| body.find(s))
        .min()
        .unwrap_or(body.len());
    let numbers = &body[cut..];
    let value = |key: &str, within: &str| -> Option<f64> {
        let rest = &within[within.find(key)? + key.len()..];
        let end = rest.find([' ', ')']).unwrap_or(rest.len());
        rest[..end].parse().ok()
    };
    let (estimate, actual) = match numbers.find("(actual time=") {
        Some(i) => (&numbers[..i], &numbers[i..]),
        None => (numbers, ""),
    };
    let loops = value("loops=", actual);
    // `actual time=first..last`: per-loop milliseconds to the last row.
    let last_ms = actual
        .strip_prefix("(actual time=")
        .and_then(|t| t.split_once(".."))
        .and_then(|(_, rest)| rest.split([' ', ')']).next()?.parse::<f64>().ok());
    PlanNode {
        label: body[..cut].trim().to_owned(),
        detail: None,
        cost: value("cost=", estimate),
        rows: value("rows=", estimate),
        actual_rows: value("rows=", actual),
        loops,
        total_ms: last_ms.map(|ms| ms * loops.unwrap_or(1.0)),
        children: Vec::new(),
    }
}

/// `EXPLAIN QUERY PLAN` rows: `(id, parent, detail)`, parent 0 for roots.
pub fn parse_sqlite(entries: &[(i64, i64, String)]) -> Plan {
    fn build(parent: i64, entries: &[(i64, i64, String)]) -> Vec<PlanNode> {
        entries
            .iter()
            .filter(|(id, p, _)| *p == parent && *id != parent)
            .map(|(id, _, detail)| PlanNode {
                label: detail.clone(),
                children: build(*id, entries),
                ..PlanNode::default()
            })
            .collect()
    }
    let mut plan = Plan::default();
    for root in build(0, entries) {
        flatten(&root, &mut plan.rows);
    }
    for row in &mut plan.rows {
        let label = &row.node.label;
        // A full scan (no index) or a temporary sort / distinct b-tree.
        row.slow = (label.starts_with("SCAN ") && !label.contains(" USING "))
            || label.contains("USE TEMP B-TREE");
    }
    plan
}

fn flatten(node: &PlanNode, out: &mut Vec<PlanRow>) {
    fn walk(node: &PlanNode, depth: usize, out: &mut Vec<PlanRow>) {
        let children_ms: Option<f64> = node.children.iter().map(|c| c.total_ms).sum();
        let self_ms = match (node.total_ms, children_ms) {
            (Some(total), Some(children)) => Some((total - children).max(0.0)),
            (Some(total), None) if node.children.is_empty() => Some(total),
            _ => None,
        };
        let mut shallow = node.clone();
        shallow.children = Vec::new();
        out.push(PlanRow {
            depth,
            node: shallow,
            self_ms,
            slow: false,
        });
        for child in &node.children {
            walk(child, depth + 1, out);
        }
    }
    walk(node, 0, out);
}

/// Flags nodes whose own time (or, without timings, own cost) is at least
/// [`SLOW_SHARE`] of the plan's.
fn mark_slow(rows: &mut [PlanRow]) {
    let total_ms: f64 = rows
        .iter()
        .filter(|r| r.depth == 0)
        .filter_map(|r| r.node.total_ms)
        .sum();
    if total_ms > 0.0 {
        for row in rows.iter_mut() {
            row.slow = row
                .self_ms
                .is_some_and(|ms| ms >= SLOW_MIN_MS && ms >= total_ms * SLOW_SHARE);
        }
        return;
    }
    let root_cost: f64 = rows
        .iter()
        .filter(|r| r.depth == 0)
        .filter_map(|r| r.node.cost)
        .sum();
    if root_cost <= 0.0 {
        return;
    }
    // Own cost: the node's cost minus its direct children's.
    for i in 0..rows.len() {
        let Some(cost) = rows[i].node.cost else {
            continue;
        };
        let depth = rows[i].depth;
        let children: f64 = rows[i + 1..]
            .iter()
            .take_while(|r| r.depth > depth)
            .filter(|r| r.depth == depth + 1)
            .filter_map(|r| r.node.cost)
            .sum();
        rows[i].slow = (cost - children).max(0.0) >= root_cost * SLOW_SHARE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PG_ANALYZED: &str = r#"[{"Plan": {"Node Type": "Hash Join", "Join Type": "Inner",
        "Total Cost": 100.0, "Plan Rows": 10, "Actual Total Time": 50.0, "Actual Rows": 8,
        "Actual Loops": 1, "Hash Cond": "(o.customer_id = c.id)",
        "Plans": [
          {"Node Type": "Seq Scan", "Relation Name": "orders", "Alias": "o", "Total Cost": 80.0,
           "Plan Rows": 1000, "Actual Total Time": 40.0, "Actual Rows": 1000, "Actual Loops": 1,
           "Filter": "(total > 10)"},
          {"Node Type": "Index Scan", "Index Name": "customer_pkey", "Relation Name": "customer",
           "Alias": "c", "Total Cost": 8.0, "Plan Rows": 1, "Actual Total Time": 0.5,
           "Actual Rows": 1, "Actual Loops": 4}
        ]}, "Planning Time": 0.2, "Execution Time": 50.3}]"#;

    #[test]
    fn postgres_json_plans_become_a_tree_with_self_times() {
        let plan = parse_postgres(PG_ANALYZED).expect("plan");
        assert_eq!(plan.execution_ms, Some(50.3));
        let labels: Vec<(usize, &str)> = plan
            .rows
            .iter()
            .map(|r| (r.depth, r.node.label.as_str()))
            .collect();
        assert_eq!(
            labels,
            vec![
                (0, "Hash Join (Inner)"),
                (1, "Seq Scan on orders o"),
                (1, "Index Scan using customer_pkey on customer c"),
            ]
        );
        // Loops multiply per-loop times: the index scan took 2 ms in all.
        assert_eq!(plan.rows[2].node.total_ms, Some(2.0));
        assert_eq!(plan.rows[0].self_ms, Some(8.0));
        let slow: Vec<bool> = plan.rows.iter().map(|r| r.slow).collect();
        assert_eq!(slow, vec![false, true, false]);
        assert_eq!(
            plan.rows[1].node.detail.as_deref(),
            Some("Filter: (total > 10)")
        );
    }

    #[test]
    fn estimates_only_flag_the_dominant_cost() {
        let json = r#"[{"Plan": {"Node Type": "Sort", "Total Cost": 100.0, "Plan Rows": 5,
            "Sort Key": ["a", "b"],
            "Plans": [{"Node Type": "Seq Scan", "Relation Name": "t", "Total Cost": 90.0}]}}]"#;
        let plan = parse_postgres(json).expect("plan");
        assert_eq!(plan.rows[0].node.detail.as_deref(), Some("Sort Key: a, b"));
        assert!(!plan.rows[0].slow && plan.rows[1].slow);
        assert!(parse_postgres("not json").is_err());
    }

    #[test]
    fn mysql_tree_with_actuals() {
        let text = "\
-> Nested loop inner join  (cost=2.6 rows=3) (actual time=0.05..12.5 rows=3 loops=1)
    -> Table scan on c  (cost=0.55 rows=3) (actual time=0.02..10.1 rows=3 loops=1)
    -> Single-row index lookup on o using PRIMARY (id=c.id)  (cost=0.28 rows=1) (actual time=0.01..0.2 rows=1 loops=3)
-> Something else  (never executed)";
        let plan = parse_mysql_tree(text).expect("plan");
        let rows: Vec<(usize, &str, Option<f64>, Option<f64>)> = plan
            .rows
            .iter()
            .map(|r| (r.depth, r.node.label.as_str(), r.node.cost, r.node.total_ms))
            .collect();
        assert_eq!(
            rows,
            vec![
                (0, "Nested loop inner join", Some(2.6), Some(12.5)),
                (1, "Table scan on c", Some(0.55), Some(10.1)),
                (
                    1,
                    "Single-row index lookup on o using PRIMARY (id=c.id)",
                    Some(0.28),
                    Some(0.6000000000000001)
                ),
                (0, "Something else", None, None),
            ]
        );
        assert_eq!(plan.rows[2].node.loops, Some(3.0));
        assert!(plan.rows[1].slow && !plan.rows[0].slow);
    }

    #[test]
    fn sqlite_query_plan_rows_nest_by_parent() {
        let plan = parse_sqlite(&[
            (2, 0, "SCAN o".into()),
            (5, 0, "SEARCH c USING INTEGER PRIMARY KEY (rowid=?)".into()),
            (9, 0, "USE TEMP B-TREE FOR ORDER BY".into()),
            (12, 5, "child".into()),
        ]);
        let rows: Vec<(usize, &str, bool)> = plan
            .rows
            .iter()
            .map(|r| (r.depth, r.node.label.as_str(), r.slow))
            .collect();
        assert_eq!(
            rows,
            vec![
                (0, "SCAN o", true),
                (0, "SEARCH c USING INTEGER PRIMARY KEY (rowid=?)", false),
                (1, "child", false),
                (0, "USE TEMP B-TREE FOR ORDER BY", true),
            ]
        );
    }

    #[test]
    fn explain_statements_per_engine() {
        assert_eq!(
            explain_sql(DatabaseEngine::Postgres, "select 1;", true).as_deref(),
            Ok("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) select 1")
        );
        assert_eq!(
            explain_sql(DatabaseEngine::MySql, "select 1", false).as_deref(),
            Ok("EXPLAIN FORMAT=TREE select 1")
        );
        assert!(explain_sql(DatabaseEngine::Sqlite, "select 1", true).is_err());
        assert!(explain_sql(DatabaseEngine::MongoDb, "x", false).is_err());
    }
}
