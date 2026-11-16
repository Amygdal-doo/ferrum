use ferrumdata_application::edits::RowEdits;
use ferrumdata_domain::{
    CellValue, DbError, ExecutionId, ProfileId, QueryEvent, QueryOutcome, QueryState,
};
use ferrumdata_result_grid_core::{
    ExportFormat, GridSelection, MAX_RESIZED_WIDTH, MIN_RESIZED_WIDTH, ResultBuffer, SortDirection,
    WIDTH_SAMPLE_ROWS, estimate_widths, export_string, sorted_order,
};
use std::time::{Duration, Instant};

/// The latest execution for one editor tab.
#[derive(Debug, Clone, PartialEq)]
pub struct TabRun {
    pub execution_id: ExecutionId,
    pub profile_id: ProfileId,
    pub sql: String,
    pub state: QueryState,
    pub buffer: ResultBuffer,
    pub outcome: Option<QueryOutcome>,
    pub error: Option<DbError>,
    pub messages: Vec<String>,
    pub started_at: Instant,
    pub elapsed: Option<Duration>,
    /// Pixel widths per column, estimated from the first rows.
    pub col_widths: Vec<f64>,
    /// Set once the user resizes a column: estimates no longer overwrite widths.
    pub widths_pinned: bool,
    /// Informational note shown above the grid (e.g. non-deterministic order).
    pub note: Option<String>,
    /// Client-side sort: column and direction.
    pub sort: Option<(usize, SortDirection)>,
    /// View position → buffer row while sorted.
    order: Option<Vec<usize>>,
    /// In view coordinates.
    pub selection: Option<GridSelection>,
    /// Row edits staged on a table tab's result (buffer row indexes).
    pub edits: RowEdits,
}

impl TabRun {
    #[must_use]
    pub fn new(
        execution_id: ExecutionId,
        profile_id: ProfileId,
        sql: String,
        max_rows: usize,
    ) -> Self {
        Self {
            execution_id,
            profile_id,
            sql,
            state: QueryState::Created,
            buffer: ResultBuffer::new(max_rows),
            outcome: None,
            error: None,
            messages: Vec::new(),
            started_at: Instant::now(),
            elapsed: None,
            col_widths: Vec::new(),
            widths_pinned: false,
            note: None,
            sort: None,
            order: None,
            selection: None,
            edits: RowEdits::default(),
        }
    }

    /// Applies one event; invalid transitions (e.g. after a terminal state) are dropped.
    pub fn apply(&mut self, event: QueryEvent) {
        let Some(next) = self.state.apply(&event) else {
            tracing::debug!(execution = %self.execution_id, "ignoring out-of-order query event");
            return;
        };
        self.state = next;
        match event {
            QueryEvent::Started => {}
            QueryEvent::Metadata(columns) => {
                self.col_widths = estimate_widths(&columns, std::iter::empty());
                self.buffer.set_columns(columns);
            }
            QueryEvent::Rows(rows) => {
                let sampled_before = self.buffer.len();
                self.buffer.push_batch(rows);
                // Re-estimate only while still inside the sample window.
                if sampled_before < WIDTH_SAMPLE_ROWS && !self.widths_pinned {
                    let sample = self.buffer.rows(0..WIDTH_SAMPLE_ROWS).map(|(_, r)| r);
                    self.col_widths = estimate_widths(self.buffer.columns(), sample);
                }
                // Rows streamed in after a header click join the sorted order.
                self.resort();
            }
            QueryEvent::Message(m) => self.messages.push(m),
            QueryEvent::Completed(outcome) => {
                if outcome.truncated {
                    self.buffer.mark_truncated();
                }
                self.elapsed = Some(outcome.elapsed);
                self.outcome = Some(outcome);
            }
            QueryEvent::Cancelled { elapsed } | QueryEvent::TimedOut { elapsed } => {
                self.elapsed = Some(elapsed);
            }
            QueryEvent::Failed(e) => {
                self.elapsed = Some(self.started_at.elapsed());
                self.error = Some(e);
            }
        }
    }

    /// Header click: cycles the column through ascending, descending, unsorted.
    /// The selection is cleared because view positions change.
    pub fn toggle_sort(&mut self, col: usize) {
        let current = self.sort.filter(|(c, _)| *c == col).map(|(_, d)| d);
        self.sort = SortDirection::next(current).map(|d| (col, d));
        self.selection = None;
        self.resort();
    }

    fn resort(&mut self) {
        self.order = self
            .sort
            .map(|(col, direction)| sorted_order(&self.buffer, col, direction));
    }

    /// Buffer row shown at view position `index` (`None` past the buffer,
    /// where staged new rows are listed).
    #[must_use]
    pub fn buffer_index(&self, index: usize) -> Option<usize> {
        match &self.order {
            Some(order) => order.get(index).copied(),
            None => (index < self.buffer.len()).then_some(index),
        }
    }

    /// Rows the grid lists: the result, then staged new rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.buffer.len() + self.edits.new_rows().len()
    }

    /// The row shown at view position `index`.
    #[must_use]
    pub fn view_row(&self, index: usize) -> Option<&[CellValue]> {
        match &self.order {
            Some(order) => order.get(index).and_then(|&i| self.buffer.row(i)),
            None => self.buffer.row(index),
        }
    }

    /// Rows at view positions `range` (clamped), in display order.
    pub fn view_rows(
        &self,
        range: std::ops::Range<usize>,
    ) -> impl Iterator<Item = (usize, &[CellValue])> {
        let end = range.end.min(self.buffer.len());
        let start = range.start.min(end);
        (start..end).filter_map(|i| self.view_row(i).map(|r| (i, r)))
    }

    /// Width set by dragging a header edge; pins all widths from then on.
    pub fn resize_column(&mut self, col: usize, width: f64) {
        if let Some(w) = self.col_widths.get_mut(col) {
            *w = width.clamp(MIN_RESIZED_WIDTH, MAX_RESIZED_WIDTH);
            self.widths_pinned = true;
        }
    }

    /// The selection, or every cell when nothing is selected, as
    /// (view row range, column indices). `None` for an empty grid.
    #[must_use]
    pub fn scope(&self, selection_only: bool) -> Option<(std::ops::Range<usize>, Vec<usize>)> {
        let cols = self.buffer.columns().len();
        let selection = match self.selection {
            Some(s) if selection_only => s,
            _ => GridSelection::all(self.buffer.len(), cols)?,
        };
        let rows = selection.rows();
        let rows = rows.start.min(self.buffer.len())..rows.end.min(self.buffer.len());
        Some((rows, selection.cols(cols).collect()))
    }

    /// The selected cells (or the whole grid) in `format`, in display order.
    #[must_use]
    pub fn export_text(&self, format: ExportFormat, selection_only: bool) -> Option<String> {
        let (rows, cols) = self.scope(selection_only)?;
        Some(export_string(
            format,
            self.buffer.columns(),
            &cols,
            self.view_rows(rows).map(|(_, r)| r),
        ))
    }

    /// ⌘C: one cell as its full text, several cells as TSV.
    #[must_use]
    pub fn copy_text(&self) -> Option<String> {
        let selection = self.selection?;
        if selection.is_single_cell() {
            let cell = self
                .view_row(selection.anchor.row)?
                .get(selection.anchor.col)?;
            return Some(ferrumdata_result_grid_core::cell_text(cell).into_owned());
        }
        self.export_text(ExportFormat::Tsv, true)
    }

    /// Marks a run that failed before the driver produced any event.
    pub fn fail(&mut self, error: DbError) {
        self.apply(QueryEvent::Failed(error));
    }

    #[must_use]
    pub fn status_label(&self) -> String {
        let ms = |d: Duration| format!("{} ms", d.as_millis());
        match self.state {
            QueryState::Created | QueryState::Running => "Running…".into(),
            QueryState::Streaming => format!("Fetching… {} rows", self.buffer.len()),
            QueryState::Completed => match &self.outcome {
                Some(o) if o.rows_affected.is_some() && self.buffer.columns().is_empty() => {
                    format!(
                        "{} rows affected · {}",
                        o.rows_affected.unwrap_or(0),
                        ms(o.elapsed)
                    )
                }
                Some(o) => {
                    let more = if o.truncated { " (limit reached)" } else { "" };
                    format!("{} rows{more} · {}", self.buffer.len(), ms(o.elapsed))
                }
                None => "Completed".into(),
            },
            QueryState::Cancelled => {
                format!("Cancelled · {}", self.elapsed.map(ms).unwrap_or_default())
            }
            QueryState::TimedOut => {
                format!("Timed out · {}", self.elapsed.map(ms).unwrap_or_default())
            }
            QueryState::Failed => "Failed".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{ColumnMeta, LogicalType};

    fn run_with(rows: &[(i64, &str)]) -> TabRun {
        let col = |name: &str, ty| ColumnMeta {
            name: name.into(),
            db_type: "t".into(),
            logical_type: ty,
            nullable: None,
            primary_key: false,
            editable: false,
        };
        let mut run = TabRun::new(ExecutionId::new(), ProfileId::new(), "q".into(), 100);
        run.apply(QueryEvent::Started);
        run.apply(QueryEvent::Metadata(vec![
            col("id", LogicalType::Integer),
            col("name", LogicalType::Text),
        ]));
        run.apply(QueryEvent::Rows(
            rows.iter()
                .map(|(id, name)| vec![CellValue::Signed(*id), CellValue::Text((*name).into())])
                .collect(),
        ));
        run
    }

    fn ids(run: &TabRun) -> Vec<i64> {
        run.view_rows(0..usize::MAX)
            .filter_map(|(_, r)| match r[0] {
                CellValue::Signed(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn header_clicks_sort_and_restore_order_and_streamed_rows_join_the_sort() {
        let mut run = run_with(&[(2, "b"), (10, "a"), (1, "c")]);
        run.selection = Some(GridSelection::cell(0, 0));
        run.toggle_sort(0);
        assert_eq!(ids(&run), vec![1, 2, 10]);
        assert_eq!(run.selection, None, "view positions changed");
        run.apply(QueryEvent::Rows(vec![vec![
            CellValue::Signed(5),
            CellValue::Text("d".into()),
        ]]));
        assert_eq!(ids(&run), vec![1, 2, 5, 10]);
        run.toggle_sort(0);
        assert_eq!(ids(&run), vec![10, 5, 2, 1]);
        run.toggle_sort(0);
        assert_eq!(ids(&run), vec![2, 10, 1, 5]);
        // Another column starts again at ascending.
        run.toggle_sort(0);
        run.toggle_sort(1);
        assert_eq!(run.sort, Some((1, SortDirection::Ascending)));
        assert_eq!(ids(&run), vec![10, 2, 1, 5]);
    }

    #[test]
    fn copy_uses_display_order_and_selection() {
        let mut run = run_with(&[(2, "b"), (1, "line\nbreak")]);
        run.toggle_sort(0);
        run.selection = Some(GridSelection::cell(0, 1));
        assert_eq!(run.copy_text().as_deref(), Some("line\nbreak"));
        run.selection = Some(GridSelection::row(1));
        assert_eq!(run.copy_text().as_deref(), Some("2\tb\n"));
        run.selection = Some(GridSelection::cell(0, 0).extend_to(1, 1));
        assert_eq!(
            run.export_text(ExportFormat::Csv, true).as_deref(),
            Some("id,name\r\n1,\"line\nbreak\"\r\n2,b\r\n")
        );
        // Whole grid regardless of selection.
        run.selection = Some(GridSelection::cell(1, 1));
        assert_eq!(
            run.export_text(ExportFormat::Csv, false).as_deref(),
            Some("id,name\r\n1,\"line\nbreak\"\r\n2,b\r\n")
        );
        run.selection = None;
        assert_eq!(run.copy_text(), None);
    }

    #[test]
    fn staged_new_rows_follow_the_result_in_any_sort_order() {
        let mut run = run_with(&[(2, "b"), (1, "a")]);
        run.edits.add_row(2);
        assert_eq!(run.row_count(), 3);
        assert_eq!(run.buffer_index(0), Some(0));
        assert_eq!(run.buffer_index(2), None);
        run.toggle_sort(0);
        assert_eq!(run.buffer_index(0), Some(1));
        assert_eq!(run.buffer_index(2), None);
    }

    #[test]
    fn resized_widths_are_clamped_and_survive_more_rows() {
        let mut run = run_with(&[(1, "a")]);
        run.resize_column(1, 5.0);
        assert_eq!(run.col_widths[1], MIN_RESIZED_WIDTH);
        run.resize_column(1, 500.0);
        run.apply(QueryEvent::Rows(vec![vec![
            CellValue::Signed(2),
            CellValue::Text("much longer text value".into()),
        ]]));
        assert_eq!(run.col_widths[1], 500.0);
        run.resize_column(9, 100.0); // out of range: ignored
        assert_eq!(run.col_widths.len(), 2);
    }
}
