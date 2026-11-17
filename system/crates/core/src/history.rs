//! Undo and redo: the states before each edit. Edits of one kind in a row
//! (typing a component's name, a key per change) can share a group and
//! then undo as one.

/// How many edits can be undone.
const LIMIT: usize = 200;

#[derive(Debug, Clone)]
pub struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    /// The group of the last edit, while further edits may join it.
    group: Option<String>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            group: None,
        }
    }
}

impl<T> History<T> {
    /// Notes an edit made from `before`. An edit in the same `group` as the
    /// one just before joins it (its `before` is already kept).
    pub fn record(&mut self, before: T, group: Option<&str>) {
        self.redo.clear();
        if group.is_some() && group == self.group.as_deref() && !self.undo.is_empty() {
            return;
        }
        self.group = group.map(str::to_owned);
        self.undo.push(before);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
    }

    /// Ends the current group: the next edit starts its own.
    pub fn seal(&mut self) {
        self.group = None;
    }

    /// The state before the last edit, swapped with `current`.
    pub fn undo(&mut self, current: T) -> Option<T> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        self.group = None;
        Some(previous)
    }

    /// The state the last undo left, swapped with `current`.
    pub fn redo(&mut self, current: T) -> Option<T> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        self.group = None;
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undoes_and_redoes_with_typing_grouped() {
        let mut h = History::default();
        let mut state = "a".to_string();
        let edit = |h: &mut History<String>, state: &mut String, new: &str, g| {
            h.record(std::mem::replace(state, new.into()), g);
        };
        edit(&mut h, &mut state, "ab", Some("name"));
        edit(&mut h, &mut state, "abc", Some("name"));
        edit(&mut h, &mut state, "abc!", None);
        assert_eq!(h.undo(state.clone()).as_deref(), Some("abc"));
        assert_eq!(
            h.undo("abc".into()).as_deref(),
            Some("a"),
            "typing undoes at once"
        );
        assert!(!h.can_undo());
        assert_eq!(h.redo("a".into()).as_deref(), Some("abc"));
        assert_eq!(h.redo("abc".into()).as_deref(), Some("abc!"));
        assert!(h.redo("abc!".into()).is_none());

        // A new edit drops what could be redone; a sealed group starts anew.
        state = "x".into();
        h.clear();
        edit(&mut h, &mut state, "xy", Some("name"));
        h.seal();
        edit(&mut h, &mut state, "xyz", Some("name"));
        assert_eq!(h.undo(state.clone()).as_deref(), Some("xy"));
        edit(&mut h, &mut state, "q", None);
        assert!(!h.can_redo());
    }
}
