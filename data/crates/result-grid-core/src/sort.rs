//! Client-side, type-aware sorting of a loaded result set.
//!
//! Sorting never re-runs the query: it computes a permutation of the rows in
//! the buffer. Numbers compare by value across integer/decimal/float cells,
//! temporal values chronologically, text case-insensitively. `NULL` (and
//! fields absent from a document) always sort last, in both directions.

use crate::ResultBuffer;
use chrono::{NaiveDateTime, NaiveTime};
use ferrumdata_domain::CellValue;
use std::cmp::Ordering;

/// Type name the document drivers use for a field a document lacks
/// (`ferrumdata_db_driver_api::documents::ABSENT`).
pub const ABSENT_TYPE: &str = "absent";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    /// Header click cycle: unsorted → ascending → descending → unsorted.
    #[must_use]
    pub const fn next(current: Option<Self>) -> Option<Self> {
        match current {
            None => Some(Self::Ascending),
            Some(Self::Ascending) => Some(Self::Descending),
            Some(Self::Descending) => None,
        }
    }
}

/// Row indices of `buffer` ordered by column `col`. Stable: equal values
/// keep their original order.
#[must_use]
pub fn sorted_order(buffer: &ResultBuffer, col: usize, direction: SortDirection) -> Vec<usize> {
    let cell = |i: usize| buffer.row(i).and_then(|r| r.get(col));
    let mut order: Vec<usize> = (0..buffer.len()).collect();
    order.sort_by(|&a, &b| match (cell(a), cell(b)) {
        (Some(x), Some(y)) => compare_for_sort(x, y, direction),
        (x, y) => x.is_none().cmp(&y.is_none()),
    });
    order
}

/// Like [`compare_cells`] with the direction applied, keeping empty cells last.
#[must_use]
pub fn compare_for_sort(a: &CellValue, b: &CellValue, direction: SortDirection) -> Ordering {
    match (is_empty(a), is_empty(b)) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => match direction {
            SortDirection::Ascending => compare_cells(a, b),
            SortDirection::Descending => compare_cells(a, b).reverse(),
        },
    }
}

/// `NULL` or a document field that is absent.
#[must_use]
pub fn is_empty(cell: &CellValue) -> bool {
    match cell {
        CellValue::Null => true,
        CellValue::Unsupported(ty) => ty == ABSENT_TYPE,
        _ => false,
    }
}

/// Ascending order of two cells. Values of different kinds (possible in
/// SQLite and document results) order by kind: numbers, booleans, dates and
/// timestamps, times, text, binary, undecodable, `NULL`.
#[must_use]
pub fn compare_cells(a: &CellValue, b: &CellValue) -> Ordering {
    use CellValue as C;
    let (ra, rb) = (rank(a), rank(b));
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (C::Bool(x), C::Bool(y)) => x.cmp(y),
        (C::Time(x), C::Time(y)) => x.cmp(y),
        (C::Binary(x), C::Binary(y)) => x.cmp(y),
        (C::Unsupported(x), C::Unsupported(y)) => x.cmp(y),
        _ if ra == 0 => compare_numbers(a, b),
        _ if ra == 2 => instant(a).cmp(&instant(b)),
        _ => match (text_of(a), text_of(b)) {
            (Some(x), Some(y)) => compare_text(x, y),
            _ => Ordering::Equal,
        },
    }
}

fn rank(cell: &CellValue) -> u8 {
    use CellValue as C;
    match cell {
        C::Signed(_) | C::Unsigned(_) | C::Float(_) | C::Decimal(_) => 0,
        C::Bool(_) => 1,
        C::Date(_) | C::Timestamp(_) | C::TimestampTz(_) => 2,
        C::Time(_) => 3,
        C::Text(_) | C::Uuid(_) | C::Json(_) => 4,
        C::Binary(_) => 5,
        C::Unsupported(_) => 6,
        C::Null => 7,
    }
}

/// Timestamps without a zone are compared as if they were UTC.
fn instant(cell: &CellValue) -> Option<NaiveDateTime> {
    match cell {
        CellValue::Date(d) => Some(d.and_time(NaiveTime::MIN)),
        CellValue::Timestamp(ts) => Some(*ts),
        CellValue::TimestampTz(ts) => Some(ts.naive_utc()),
        _ => None,
    }
}

fn text_of(cell: &CellValue) -> Option<&str> {
    match cell {
        CellValue::Text(s) | CellValue::Uuid(s) | CellValue::Json(s) => Some(s),
        _ => None,
    }
}

/// Case-insensitive, then case-sensitive as a tie-break so the order is total.
fn compare_text(a: &str, b: &str) -> Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
        .then_with(|| a.cmp(b))
}

/// Exact for integers and plain decimals; falls back to `f64` when a float
/// or a non-plain decimal (`NaN`, exponent) is involved. NaN sorts last.
fn compare_numbers(a: &CellValue, b: &CellValue) -> Ordering {
    use CellValue as C;
    let int = |c: &CellValue| match c {
        C::Signed(v) => Some(i128::from(*v)),
        C::Unsigned(v) => Some(i128::from(*v)),
        _ => None,
    };
    if let (Some(x), Some(y)) = (int(a), int(b)) {
        return x.cmp(&y);
    }
    if !matches!(a, C::Float(_)) && !matches!(b, C::Float(_)) {
        let text = |c: &CellValue| match c {
            C::Decimal(s) => s.clone(),
            other => int(other).map(|v| v.to_string()).unwrap_or_default(),
        };
        let (ta, tb) = (text(a), text(b));
        if let (Some(x), Some(y)) = (PlainDecimal::parse(&ta), PlainDecimal::parse(&tb)) {
            return x.cmp(&y);
        }
    }
    let float = |c: &CellValue| match c {
        C::Float(v) => *v,
        C::Decimal(s) => s.trim().parse().unwrap_or(f64::NAN),
        other => int(other).map_or(f64::NAN, |v| v as f64),
    };
    let (x, y) = (float(a), float(b));
    match (x.is_nan(), y.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
    }
}

/// `[+-]digits[.digits]` with leading/trailing zeros normalised away.
#[derive(Debug, PartialEq, Eq)]
struct PlainDecimal<'a> {
    negative: bool,
    int: &'a str,
    frac: &'a str,
}

impl<'a> PlainDecimal<'a> {
    fn parse(s: &'a str) -> Option<Self> {
        let s = s.trim();
        let (negative, digits) = match s.as_bytes().first()? {
            b'-' => (true, &s[1..]),
            b'+' => (false, &s[1..]),
            _ => (false, s),
        };
        let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
        let all_digits = |p: &str| p.bytes().all(|c| c.is_ascii_digit());
        if (int.is_empty() && frac.is_empty()) || !all_digits(int) || !all_digits(frac) {
            return None;
        }
        let int = int.trim_start_matches('0');
        let frac = frac.trim_end_matches('0');
        // -0 equals 0.
        let negative = negative && !(int.is_empty() && frac.is_empty());
        Some(Self {
            negative,
            int,
            frac,
        })
    }

    fn cmp_magnitude(&self, other: &Self) -> Ordering {
        self.int
            .len()
            .cmp(&other.int.len())
            .then_with(|| self.int.cmp(other.int))
            .then_with(|| self.frac.cmp(other.frac))
    }
}

impl Ord for PlainDecimal<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => self.cmp_magnitude(other),
            (true, true) => other.cmp_magnitude(self),
        }
    }
}

impl PartialOrd for PlainDecimal<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate};
    use ferrumdata_domain::{ColumnMeta, LogicalType};

    fn dec(s: &str) -> CellValue {
        CellValue::Decimal(s.into())
    }

    fn sorted(cells: Vec<CellValue>, direction: SortDirection) -> Vec<CellValue> {
        let mut buffer = ResultBuffer::new(100);
        buffer.set_columns(vec![ColumnMeta {
            name: "v".into(),
            db_type: "any".into(),
            logical_type: LogicalType::Other,
            nullable: None,
            primary_key: false,
            editable: false,
        }]);
        buffer.push_batch(cells.into_iter().map(|c| vec![c]).collect());
        sorted_order(&buffer, 0, direction)
            .into_iter()
            .filter_map(|i| buffer.row(i).map(|r| r[0].clone()))
            .collect()
    }

    #[test]
    fn numbers_compare_by_value_not_text() {
        let got = sorted(
            vec![
                CellValue::Signed(10),
                CellValue::Signed(9),
                CellValue::Signed(-100),
                CellValue::Unsigned(u64::MAX),
                CellValue::Float(9.5),
            ],
            SortDirection::Ascending,
        );
        assert_eq!(
            got,
            vec![
                CellValue::Signed(-100),
                CellValue::Signed(9),
                CellValue::Float(9.5),
                CellValue::Signed(10),
                CellValue::Unsigned(u64::MAX),
            ]
        );
    }

    #[test]
    fn decimals_are_compared_exactly() {
        // These differ beyond f64 precision.
        let a = dec("12345678901234567890.000000000000000001");
        let b = dec("12345678901234567890.000000000000000002");
        assert_eq!(compare_cells(&a, &b), Ordering::Less);
        assert_eq!(compare_cells(&dec("0.50"), &dec("0.5")), Ordering::Equal);
        assert_eq!(compare_cells(&dec("-0"), &dec("0.000")), Ordering::Equal);
        assert_eq!(compare_cells(&dec("-2.5"), &dec("-10")), Ordering::Greater);
        assert_eq!(compare_cells(&dec("0.05"), &dec(".5")), Ordering::Less);
        assert_eq!(
            compare_cells(&CellValue::Signed(3), &dec("2.99")),
            Ordering::Greater
        );
        // Non-plain forms fall back to floats; NaN sorts after every number.
        assert_eq!(compare_cells(&dec("1E+3"), &dec("999")), Ordering::Greater);
        assert_eq!(compare_cells(&dec("NaN"), &dec("1e308")), Ordering::Greater);
        assert_eq!(
            compare_cells(&CellValue::Float(f64::NAN), &CellValue::Float(1.0)),
            Ordering::Greater
        );
    }

    #[test]
    fn nulls_and_absent_fields_sort_last_in_both_directions() {
        let cells = vec![
            CellValue::Null,
            CellValue::Signed(2),
            CellValue::Unsupported(ABSENT_TYPE.into()),
            CellValue::Signed(1),
        ];
        let asc = sorted(cells.clone(), SortDirection::Ascending);
        assert_eq!(&asc[..2], &[CellValue::Signed(1), CellValue::Signed(2)]);
        assert!(asc[2..].iter().all(is_empty));
        let desc = sorted(cells, SortDirection::Descending);
        assert_eq!(&desc[..2], &[CellValue::Signed(2), CellValue::Signed(1)]);
        assert!(desc[2..].iter().all(is_empty));
    }

    #[test]
    fn dates_and_timestamps_are_chronological_across_kinds() {
        let day = |d| NaiveDate::from_ymd_opt(2024, 1, d).expect("date");
        let ts = |d, h| day(d).and_hms_opt(h, 0, 0).expect("ts");
        let tz = DateTime::parse_from_rfc3339("2024-01-02T03:00:00+05:00").expect("tz");
        let got = sorted(
            vec![
                CellValue::Timestamp(ts(2, 12)),
                CellValue::TimestampTz(tz), // 2024-01-01T22:00Z
                CellValue::Date(day(2)),
                CellValue::Date(day(1)),
            ],
            SortDirection::Ascending,
        );
        assert_eq!(
            got,
            vec![
                CellValue::Date(day(1)),
                CellValue::TimestampTz(tz),
                CellValue::Date(day(2)),
                CellValue::Timestamp(ts(2, 12)),
            ]
        );
    }

    #[test]
    fn text_is_case_insensitive_and_stable() {
        let t = |s: &str| CellValue::Text(s.into());
        let got = sorted(
            vec![t("banana"), t("Apple"), t("apple"), t("cherry"), t("")],
            SortDirection::Ascending,
        );
        assert_eq!(
            got,
            vec![t(""), t("Apple"), t("apple"), t("banana"), t("cherry")]
        );
        // Equal values keep their original relative order.
        let mut buffer = ResultBuffer::new(10);
        buffer.push_batch(vec![
            vec![t("x"), CellValue::Signed(0)],
            vec![t("x"), CellValue::Signed(1)],
        ]);
        assert_eq!(
            sorted_order(&buffer, 0, SortDirection::Descending),
            vec![0, 1]
        );
    }

    #[test]
    fn mixed_kinds_group_by_kind() {
        let got = sorted(
            vec![
                CellValue::Text("a".into()),
                CellValue::Bool(true),
                CellValue::Signed(5),
                CellValue::Bool(false),
            ],
            SortDirection::Ascending,
        );
        assert_eq!(
            got,
            vec![
                CellValue::Signed(5),
                CellValue::Bool(false),
                CellValue::Bool(true),
                CellValue::Text("a".into()),
            ]
        );
    }

    #[test]
    fn header_clicks_cycle_through_directions() {
        let a = SortDirection::next(None);
        assert_eq!(a, Some(SortDirection::Ascending));
        let d = SortDirection::next(a);
        assert_eq!(d, Some(SortDirection::Descending));
        assert_eq!(SortDirection::next(d), None);
    }
}
