//! Editable-table helpers. Tables render one extra blank row; editing it
//! appends a real row.

use ferrumapi_domain::{FormField, FormFieldKind, KeyValue, Variable};

/// Applies `f` to row `i`, appending a default row first when `i` is the
/// trailing blank row.
pub fn edit_row<T: Default>(rows: &mut Vec<T>, i: usize, f: impl FnOnce(&mut T)) {
    if i >= rows.len() {
        rows.push(T::default());
        if let Some(row) = rows.last_mut() {
            f(row);
        }
    } else if let Some(row) = rows.get_mut(i) {
        f(row);
    }
}

pub fn remove_row<T>(rows: &mut Vec<T>, i: usize) {
    if i < rows.len() {
        rows.remove(i);
    }
}

/// One edit to a table row, applied to the latest state by the owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowEdit {
    Key(String),
    Value(String),
    Enabled(bool),
    /// Variables: masked display.
    Secret(bool),
    /// Form-data: file (`true`) or text field. Clears the value.
    File(bool),
    Remove,
}

pub trait Row: Default {
    fn set(&mut self, edit: RowEdit);
}

impl Row for KeyValue {
    fn set(&mut self, edit: RowEdit) {
        match edit {
            RowEdit::Key(k) => self.key = k,
            RowEdit::Value(v) => self.value = v,
            RowEdit::Enabled(on) => self.enabled = on,
            RowEdit::Secret(_) | RowEdit::File(_) | RowEdit::Remove => {}
        }
    }
}

impl Row for Variable {
    fn set(&mut self, edit: RowEdit) {
        match edit {
            RowEdit::Key(k) => self.key = k,
            RowEdit::Value(v) => self.value = v,
            RowEdit::Enabled(on) => self.enabled = on,
            RowEdit::Secret(on) => self.secret = on,
            RowEdit::File(_) | RowEdit::Remove => {}
        }
    }
}

impl Row for FormField {
    fn set(&mut self, edit: RowEdit) {
        match edit {
            RowEdit::Key(k) => self.key = k,
            RowEdit::Value(v) => self.value = v,
            RowEdit::Enabled(on) => self.enabled = on,
            RowEdit::File(file) => {
                let kind = if file {
                    FormFieldKind::File
                } else {
                    FormFieldKind::Text
                };
                if kind != self.kind {
                    self.kind = kind;
                    self.value.clear();
                }
            }
            RowEdit::Secret(_) | RowEdit::Remove => {}
        }
    }
}

/// Applies `edit` to row `i` (the trailing blank row when `i == len`).
pub fn apply<T: Row>(rows: &mut Vec<T>, i: usize, edit: RowEdit) {
    if edit == RowEdit::Remove {
        remove_row(rows, i);
    } else {
        edit_row(rows, i, |r| r.set(edit));
    }
}

/// Rows counted in sub-tab badges.
pub trait Countable {
    fn counts(&self) -> bool;
}

impl Countable for KeyValue {
    fn counts(&self) -> bool {
        self.is_active()
    }
}

impl Countable for FormField {
    fn counts(&self) -> bool {
        self.enabled && !self.key.trim().is_empty()
    }
}

impl Countable for Variable {
    fn counts(&self) -> bool {
        self.enabled && !self.key.trim().is_empty()
    }
}

#[must_use]
pub fn active_count<T: Countable>(rows: &[T]) -> usize {
    rows.iter().filter(|r| r.counts()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_blank_row_appends() {
        let mut rows: Vec<KeyValue> = Vec::new();
        edit_row(&mut rows, 0, |r| r.key = "a".into());
        assert_eq!(rows.len(), 1);
        edit_row(&mut rows, 0, |r| r.value = "1".into());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], KeyValue::new("a", "1"));
        edit_row(&mut rows, 1, |r| r.key = "b".into());
        assert_eq!(rows.len(), 2);
        edit_row(&mut rows, 0, |r| r.enabled = false);
        assert_eq!(active_count(&rows), 1);
    }

    #[test]
    fn apply_edits_and_switches_form_kind() {
        let mut rows: Vec<FormField> = Vec::new();
        apply(&mut rows, 0, RowEdit::Value("x".into()));
        apply(&mut rows, 0, RowEdit::File(true));
        assert_eq!(rows[0].kind, FormFieldKind::File);
        assert!(rows[0].value.is_empty());
        let mut vars: Vec<Variable> = Vec::new();
        apply(&mut vars, 0, RowEdit::Secret(true));
        assert!(vars[0].secret);
        apply(&mut vars, 0, RowEdit::Remove);
        assert!(vars.is_empty());
    }

    #[test]
    fn remove_ignores_out_of_range() {
        let mut rows = vec![KeyValue::new("a", "")];
        remove_row(&mut rows, 3);
        assert_eq!(rows.len(), 1);
        remove_row(&mut rows, 0);
        assert!(rows.is_empty());
    }
}
