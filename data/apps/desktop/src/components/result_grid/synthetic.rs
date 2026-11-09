//! Synthetic result for the grid benchmark (ADR-004); no database involved.

use crate::state::TabRun;
use ferrumdata_domain::{CellValue, ColumnMeta, ExecutionId, LogicalType, ProfileId};

/// Generated data for the grid spike.
pub fn synthetic_run(rows: usize) -> TabRun {
    let col = |name: &str, ty: LogicalType| ColumnMeta {
        name: name.into(),
        db_type: format!("{ty:?}").to_lowercase(),
        logical_type: ty,
        nullable: Some(true),
        primary_key: false,
        editable: false,
    };
    let mut run = TabRun::new(
        ExecutionId::new(),
        ProfileId::new(),
        "-- synthetic".into(),
        rows,
    );
    run.apply(ferrumdata_domain::QueryEvent::Started);
    run.apply(ferrumdata_domain::QueryEvent::Metadata(vec![
        col("id", LogicalType::Integer),
        col("email", LogicalType::Text),
        col("score", LogicalType::Float),
        col("active", LogicalType::Boolean),
        col("note", LogicalType::Text),
    ]));
    let batch = (0..rows)
        .map(|i| {
            vec![
                CellValue::Signed(i as i64 + 1),
                CellValue::Text(format!("user{i}@example.com")),
                CellValue::Float((i % 1000) as f64 / 7.0),
                CellValue::Bool(i % 3 == 0),
                if i % 10 == 0 {
                    CellValue::Null
                } else {
                    CellValue::Text(String::new())
                },
            ]
        })
        .collect();
    run.apply(ferrumdata_domain::QueryEvent::Rows(batch));
    run.apply(ferrumdata_domain::QueryEvent::Completed(
        ferrumdata_domain::QueryOutcome {
            rows_returned: rows,
            rows_affected: None,
            truncated: false,
            elapsed: std::time::Duration::ZERO,
        },
    ));
    run
}

#[cfg(test)]
mod tests {
    use super::synthetic_run;

    #[test]
    fn synthetic_run_completes_with_all_rows() {
        let run = synthetic_run(1_000);
        assert!(run.state.is_terminal());
        assert_eq!(run.buffer.len(), 1_000);
        assert_eq!(run.buffer.columns().len(), 5);
        assert_eq!(run.col_widths.len(), 5);
    }
}
