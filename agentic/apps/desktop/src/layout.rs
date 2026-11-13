//! The column layout, without pixels: which of FerrumChat, FerrumGIT and
//! FerrumTerm are shown, in what order and how wide. Saved per project in
//! `~/.ferrum/agentic/layouts.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One of the three columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Column {
    Chat,
    Git,
    Term,
}

impl Column {
    pub const ALL: [Column; 3] = [Column::Chat, Column::Git, Column::Term];

    /// The column header's title.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Column::Chat => "FerrumChat",
            Column::Git => "FerrumGIT",
            Column::Term => "FerrumTerm",
        }
    }

    /// The top-bar toggle's label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Column::Chat => "Chat",
            Column::Git => "Git",
            Column::Term => "Term",
        }
    }

    /// Element id fragment, e.g. `chat`.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Column::Chat => "chat",
            Column::Git => "git",
            Column::Term => "term",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Column> {
        Column::ALL.into_iter().find(|c| c.key() == key)
    }
}

/// A column's place in the layout.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub column: Column,
    pub visible: bool,
    /// Relative width; kept while hidden so reopening restores it.
    pub weight: f32,
}

/// Every column exactly once, in display order (hidden ones included).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub slots: Vec<Slot>,
}

/// Left or right, for "Move left" / "Move right".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

const MIN_WEIGHT: f32 = 0.05;

impl Default for Layout {
    fn default() -> Self {
        let slot = |column, weight| Slot {
            column,
            visible: true,
            weight,
        };
        Self {
            slots: vec![
                slot(Column::Chat, 0.36),
                slot(Column::Git, 0.32),
                slot(Column::Term, 0.32),
            ],
        }
    }
}

impl Layout {
    /// Repairs a layout read from disk: drops duplicates, appends missing
    /// columns, replaces unusable weights.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let mut seen = Vec::new();
        self.slots.retain(|slot| {
            let new = !seen.contains(&slot.column);
            seen.push(slot.column);
            new
        });
        for column in Column::ALL {
            if !seen.contains(&column) {
                self.slots.push(Slot {
                    column,
                    visible: true,
                    weight: 1.0 / 3.0,
                });
            }
        }
        for slot in &mut self.slots {
            if !slot.weight.is_finite() || slot.weight < MIN_WEIGHT {
                slot.weight = 1.0 / 3.0;
            }
        }
        self
    }

    fn index(&self, column: Column) -> usize {
        self.slots
            .iter()
            .position(|s| s.column == column)
            .unwrap_or_default()
    }

    /// The order of every column, hidden ones included.
    #[cfg(test)]
    #[must_use]
    pub fn order(&self) -> Vec<Column> {
        self.slots.iter().map(|s| s.column).collect()
    }

    /// The shown columns, left to right.
    #[must_use]
    pub fn visible(&self) -> Vec<Column> {
        self.slots
            .iter()
            .filter(|s| s.visible)
            .map(|s| s.column)
            .collect()
    }

    #[must_use]
    pub fn is_visible(&self, column: Column) -> bool {
        self.slots[self.index(column)].visible
    }

    /// Shows or hides `column`; returns whether anything changed.
    pub fn set_visible(&mut self, column: Column, visible: bool) -> bool {
        let index = self.index(column);
        let changed = self.slots[index].visible != visible;
        self.slots[index].visible = visible;
        changed
    }

    /// The shown columns with their share of the width (summing to 1).
    #[must_use]
    pub fn fractions(&self) -> Vec<(Column, f32)> {
        let total: f32 = self
            .slots
            .iter()
            .filter(|s| s.visible)
            .map(|s| s.weight)
            .sum();
        self.slots
            .iter()
            .filter(|s| s.visible)
            .map(|s| (s.column, s.weight / total.max(f32::EPSILON)))
            .collect()
    }

    /// The shown column next to `column` on `side`.
    #[must_use]
    pub fn neighbour(&self, column: Column, side: Side) -> Option<Column> {
        let visible = self.visible();
        let at = visible.iter().position(|c| *c == column)?;
        match side {
            Side::Left => at.checked_sub(1).map(|i| visible[i]),
            Side::Right => visible.get(at + 1).copied(),
        }
    }

    /// Swaps `column` with its shown neighbour on `side`.
    pub fn move_to_side(&mut self, column: Column, side: Side) -> bool {
        match self.neighbour(column, side) {
            Some(target) => self.move_onto(column, target),
            None => false,
        }
    }

    /// Puts `column` in `target`'s place (a header dropped on another
    /// column): before it when moving left, after it when moving right.
    pub fn move_onto(&mut self, column: Column, target: Column) -> bool {
        if column == target {
            return false;
        }
        let from = self.index(column);
        let to = self.index(target);
        let slot = self.slots.remove(from);
        self.slots.insert(to, slot);
        true
    }

    /// Moves the border on the right of `column` to `x`, a fraction of the
    /// row's width (0 = left edge). Neither neighbour gets narrower than
    /// `min` (also a fraction). Returns whether anything changed.
    pub fn resize(&mut self, column: Column, x: f32, min: f32) -> bool {
        let Some(right) = self.neighbour(column, Side::Right) else {
            return false;
        };
        let fractions = self.fractions();
        let start: f32 = fractions
            .iter()
            .take_while(|(c, _)| *c != column)
            .map(|(_, f)| f)
            .sum();
        let share = |c: Column| fractions.iter().find(|(x, _)| *x == c).map_or(0.0, |f| f.1);
        let pair = share(column) + share(right);
        let min = min.clamp(0.0, pair / 2.0);
        let left_share = (x - start).clamp(min, pair - min);
        if (left_share - share(column)).abs() < 1e-4 {
            return false;
        }
        // Re-express every shown weight as its fraction, then set the pair.
        for slot in &mut self.slots {
            if let Some((_, f)) = fractions.iter().find(|(c, _)| *c == slot.column) {
                slot.weight = *f;
            }
        }
        let (l, r) = (self.index(column), self.index(right));
        self.slots[l].weight = left_share;
        self.slots[r].weight = pair - left_share;
        true
    }
}

/// `layouts.json`: the last layout used and one per project.
#[derive(Debug, Default, Serialize, Deserialize)]
struct LayoutFile {
    #[serde(default)]
    last: Option<Layout>,
    #[serde(default)]
    projects: BTreeMap<PathBuf, Layout>,
}

/// `~/.ferrum/agentic/layouts.json`.
#[must_use]
pub fn default_file() -> Option<PathBuf> {
    ferrum_ui::project::home_dir()
        .map(|home| home.join(".ferrum").join("agentic").join("layouts.json"))
}

fn read(file: &Path) -> LayoutFile {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The layout saved for `project`, else the last one used, else the default.
#[must_use]
pub fn load(file: &Path, project: Option<&Path>) -> Layout {
    let mut saved = read(file);
    project
        .and_then(|p| saved.projects.remove(p))
        .or(saved.last)
        .unwrap_or_default()
        .normalized()
}

/// Saves `layout` for `project` (and as the last one used).
pub fn save(file: &Path, project: Option<&Path>, layout: &Layout) -> std::io::Result<()> {
    let mut saved = read(file);
    if let Some(project) = project {
        saved.projects.insert(project.to_path_buf(), layout.clone());
    }
    saved.last = Some(layout.clone());
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(&saved).map_err(std::io::Error::other)?;
    // Write then rename, so a second window never reads half a file.
    let tmp = file.with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, file)
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
