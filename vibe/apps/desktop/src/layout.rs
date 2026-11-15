//! The pane layout, without pixels: the Files pane on the far left (shown
//! or not, its width in px), the chat (always shown) and the FerrumGIT and
//! FerrumTerm panes beside it, which are shown, in what order and how wide.
//! Saved per project in `~/.ferrum/vibe/layouts.json`.

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

    /// The pane's name (tooltips, the toggles).
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Column::Chat => "FerrumChat",
            Column::Git => "FerrumGIT",
            Column::Term => "FerrumTerm",
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

/// Every column exactly once, in display order (hidden ones included),
/// and the Files pane left of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub slots: Vec<Slot>,
    /// Absent in layouts saved before the Files pane: shown, default width.
    #[serde(default)]
    pub files: FilesPane,
}

pub const DEFAULT_FILES_WIDTH: f32 = 240.;
pub const MIN_FILES_WIDTH: f32 = 160.;
pub const MAX_FILES_WIDTH: f32 = 600.;

/// The project tree on the far left.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FilesPane {
    pub visible: bool,
    /// In px; kept while hidden.
    pub width: f32,
}

impl Default for FilesPane {
    fn default() -> Self {
        Self {
            visible: true,
            width: DEFAULT_FILES_WIDTH,
        }
    }
}

impl FilesPane {
    /// `width` within the limits (a broken value gives the default).
    #[must_use]
    pub fn clamp_width(width: f32) -> f32 {
        if width.is_finite() {
            width.clamp(MIN_FILES_WIDTH, MAX_FILES_WIDTH)
        } else {
            DEFAULT_FILES_WIDTH
        }
    }

    /// Sets the width (clamped); returns whether it changed.
    pub fn resize(&mut self, width: f32) -> bool {
        let width = Self::clamp_width(width);
        let changed = (width - self.width).abs() >= 0.5;
        if changed {
            self.width = width;
        }
        changed
    }
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
                slot(Column::Chat, 0.5),
                Slot {
                    visible: false,
                    ..slot(Column::Git, 0.25)
                },
                Slot {
                    visible: false,
                    ..slot(Column::Term, 0.25)
                },
            ],
            files: FilesPane::default(),
        }
    }
}

impl Layout {
    /// Repairs a layout read from disk: drops duplicates, appends missing
    /// columns, replaces unusable weights, puts the chat first and shown.
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
        let chat = self.index(Column::Chat);
        let mut slot = self.slots.remove(chat);
        slot.visible = true;
        self.slots.insert(0, slot);
        self.files.width = FilesPane::clamp_width(self.files.width);
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

    /// Shows or hides `column` (the chat stays); returns whether anything
    /// changed.
    pub fn set_visible(&mut self, column: Column, visible: bool) -> bool {
        if column == Column::Chat && !visible {
            return false;
        }
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

    /// The shown column right of `column`.
    #[must_use]
    pub fn right_of(&self, column: Column) -> Option<Column> {
        let visible = self.visible();
        let at = visible.iter().position(|c| *c == column)?;
        visible.get(at + 1).copied()
    }

    /// Moves the border on the right of `column` to `x`, a fraction of the
    /// row's width (0 = left edge). Neither neighbour gets narrower than
    /// `min` (also a fraction). Returns whether anything changed.
    pub fn resize(&mut self, column: Column, x: f32, min: f32) -> bool {
        let Some(right) = self.right_of(column) else {
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

/// `~/.ferrum/vibe/layouts.json`.
#[must_use]
pub fn default_file() -> Option<PathBuf> {
    ferrum_ui::prefs::path("vibe/layouts.json")
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
