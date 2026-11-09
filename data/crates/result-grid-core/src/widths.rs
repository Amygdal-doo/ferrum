//! Column width estimation from the header and a sample of rows.

use ferrumdata_domain::{CellValue, ColumnMeta};

pub const MIN_COL_WIDTH: f64 = 72.0;
pub const MAX_COL_WIDTH: f64 = 360.0;
/// Bounds for a width the user set by dragging a header edge.
pub const MIN_RESIZED_WIDTH: f64 = 40.0;
pub const MAX_RESIZED_WIDTH: f64 = 2000.0;
/// Approximate monospace glyph width at 12px plus horizontal padding.
const CHAR_WIDTH: f64 = 7.4;
const PADDING: f64 = 24.0;
/// Rows inspected; keeps the cost independent of result size.
pub const WIDTH_SAMPLE_ROWS: usize = 200;

#[must_use]
pub fn estimate_widths<'a>(
    columns: &[ColumnMeta],
    rows: impl Iterator<Item = &'a [CellValue]>,
) -> Vec<f64> {
    let mut chars: Vec<usize> = columns
        .iter()
        .map(|c| c.name.chars().count().max(c.db_type.chars().count()))
        .collect();
    for row in rows.take(WIDTH_SAMPLE_ROWS) {
        for (i, cell) in row.iter().enumerate().take(chars.len()) {
            chars[i] = chars[i].max(cell.display().chars().count());
        }
    }
    chars
        .into_iter()
        .map(|n| {
            (n as f64)
                .mul_add(CHAR_WIDTH, PADDING)
                .clamp(MIN_COL_WIDTH, MAX_COL_WIDTH)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::LogicalType;

    fn col(name: &str) -> ColumnMeta {
        ColumnMeta {
            name: name.into(),
            db_type: "TEXT".into(),
            logical_type: LogicalType::Text,
            nullable: None,
            primary_key: false,
            editable: false,
        }
    }

    #[test]
    fn widths_follow_content_and_are_clamped() {
        let cols = [col("id"), col("email"), col("bio")];
        let rows = [vec![
            CellValue::Signed(1),
            CellValue::Text("someone@example.com".into()),
            CellValue::Text("x".repeat(500)),
        ]];
        let w = estimate_widths(&cols, rows.iter().map(Vec::as_slice));
        assert_eq!(w[0], MIN_COL_WIDTH);
        assert!(w[1] > MIN_COL_WIDTH && w[1] < MAX_COL_WIDTH);
        assert_eq!(w[2], MAX_COL_WIDTH);
    }
}
