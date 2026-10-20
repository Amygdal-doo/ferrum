use ferrum_domain::{CellValue, ColumnMeta, RowBatch};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Accepted,
    /// Some or all rows were dropped because the buffer is full.
    CapacityReached,
}

/// Rows of one result set, capped at `max_rows`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultBuffer {
    columns: Vec<ColumnMeta>,
    rows: Vec<Vec<CellValue>>,
    max_rows: usize,
    truncated: bool,
}

impl ResultBuffer {
    #[must_use]
    pub fn new(max_rows: usize) -> Self {
        Self {
            max_rows,
            ..Self::default()
        }
    }

    pub fn set_columns(&mut self, columns: Vec<ColumnMeta>) {
        self.columns = columns;
    }

    pub fn push_batch(&mut self, batch: RowBatch) -> PushOutcome {
        let room = self.max_rows.saturating_sub(self.rows.len());
        if batch.len() > room {
            self.rows.extend(batch.into_iter().take(room));
            self.truncated = true;
            PushOutcome::CapacityReached
        } else {
            self.rows.extend(batch);
            PushOutcome::Accepted
        }
    }

    pub fn mark_truncated(&mut self) {
        self.truncated = true;
    }

    #[must_use]
    pub fn columns(&self) -> &[ColumnMeta] {
        &self.columns
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }

    #[must_use]
    pub fn row(&self, index: usize) -> Option<&[CellValue]> {
        self.rows.get(index).map(Vec::as_slice)
    }

    /// Rows in `range`, clamped to what is loaded.
    pub fn rows(&self, range: Range<usize>) -> impl Iterator<Item = (usize, &[CellValue])> {
        let end = range.end.min(self.rows.len());
        let start = range.start.min(end);
        self.rows[start..end]
            .iter()
            .enumerate()
            .map(move |(i, r)| (start + i, r.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(n: usize) -> RowBatch {
        (0..n).map(|i| vec![CellValue::Signed(i as i64)]).collect()
    }

    #[test]
    fn caps_at_max_rows() {
        let mut b = ResultBuffer::new(5);
        assert_eq!(b.push_batch(batch(3)), PushOutcome::Accepted);
        assert_eq!(b.push_batch(batch(3)), PushOutcome::CapacityReached);
        assert_eq!(b.len(), 5);
        assert!(b.is_truncated());
        assert_eq!(b.push_batch(batch(1)), PushOutcome::CapacityReached);
        assert_eq!(b.len(), 5);
    }

    #[test]
    fn range_access_is_clamped_and_indexed() {
        let mut b = ResultBuffer::new(100);
        b.push_batch(batch(10));
        let got: Vec<usize> = b.rows(8..50).map(|(i, _)| i).collect();
        assert_eq!(got, vec![8, 9]);
        assert_eq!(b.rows(20..30).count(), 0);
    }
}
