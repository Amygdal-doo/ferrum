//! State of the tool tabs (ER diagram and the other schema tools). Tool
//! tabs are not restored after a restart.

use ferrumdata_application::er::ErLayout;
use ferrumdata_application::explain::Plan;
use ferrumdata_application::import::{CsvData, ImportPlan};
use ferrumdata_domain::{SchemaSnapshot, TableRef};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

/// What a tool tab shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolTab {
    Diagram,
    /// SQL dump of chosen tables.
    Dump,
    /// CSV import into one table (see [`ImportView::table`]).
    Import,
}

/// A long-running tool action (dump, import).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TaskStatus {
    #[default]
    Idle,
    Running,
    Done(String),
    Failed(String),
}

/// SQL dump tab: the connection's tables and what to export.
#[derive(Debug, Clone, PartialEq)]
pub struct DumpView {
    pub load: SnapshotLoad,
    /// Tables to export (all base tables once loaded).
    pub selected: BTreeSet<TableRef>,
    pub schema: bool,
    pub data: bool,
    pub status: TaskStatus,
}

impl Default for DumpView {
    fn default() -> Self {
        Self {
            load: SnapshotLoad::Loading,
            selected: BTreeSet::new(),
            schema: true,
            data: true,
            status: TaskStatus::Idle,
        }
    }
}

/// CSV import tab of one table.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportView {
    pub table: TableRef,
    /// The table's column names, loaded from the catalog.
    pub columns: Result<Vec<String>, String>,
    pub file: Option<PathBuf>,
    pub csv: Option<Arc<CsvData>>,
    pub plan: ImportPlan,
    pub status: TaskStatus,
}

impl ImportView {
    #[must_use]
    pub fn new(table: TableRef) -> Self {
        Self {
            table,
            columns: Err("Loading columns…".into()),
            file: None,
            csv: None,
            plan: ImportPlan {
                header: true,
                empty_as_null: true,
                mapping: Vec::new(),
            },
            status: TaskStatus::Idle,
        }
    }
}

/// A schema snapshot being loaded for a tool.
#[derive(Debug, Clone, PartialEq)]
pub enum SnapshotLoad {
    Loading,
    Loaded(Arc<SchemaSnapshot>),
    Failed(String),
}

/// The EXPLAIN result of a tab, shown in the results panel's Plan pane.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanLoad {
    Loading { analyze: bool },
    Loaded { plan: Plan, sql: String },
    Failed(String),
}

pub const MIN_ZOOM: f32 = 0.2;
pub const MAX_ZOOM: f32 = 2.0;

/// ER diagram of one connection: the laid-out schema plus the viewport.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagramView {
    pub load: SnapshotLoad,
    pub layout: Option<Arc<ErLayout>>,
    pub zoom: f32,
    /// Screen offset of the diagram origin within the canvas.
    pub pan: (f32, f32),
    /// Mouse position and pan when a drag started.
    pub drag: Option<((f32, f32), (f32, f32))>,
    /// Fit to the canvas on the next paint (after loading).
    pub fit_pending: bool,
}

impl Default for DiagramView {
    fn default() -> Self {
        Self {
            load: SnapshotLoad::Loading,
            layout: None,
            zoom: 1.0,
            pan: (24.0, 24.0),
            drag: None,
            fit_pending: true,
        }
    }
}

impl DiagramView {
    /// Zooms by `factor` keeping the diagram point under `anchor` (canvas
    /// coordinates) in place.
    pub fn zoom_at(&mut self, factor: f32, anchor: (f32, f32)) {
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let scale = zoom / self.zoom;
        self.pan = (
            anchor.0 - (anchor.0 - self.pan.0) * scale,
            anchor.1 - (anchor.1 - self.pan.1) * scale,
        );
        self.zoom = zoom;
    }

    /// Shows the whole diagram in a `width` × `height` canvas (never above 100 %).
    pub fn fit(&mut self, width: f32, height: f32) {
        const MARGIN: f32 = 24.0;
        let Some(layout) = &self.layout else {
            return;
        };
        let zoom = ((width - 2.0 * MARGIN) / layout.width.max(1.0))
            .min((height - 2.0 * MARGIN) / layout.height.max(1.0))
            .clamp(MIN_ZOOM, 1.0);
        self.zoom = zoom;
        self.pan = (
            ((width - layout.width * zoom) / 2.0).max(MARGIN),
            ((height - layout.height * zoom) / 2.0).max(MARGIN),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_keeps_the_anchor_point_fixed_and_is_clamped() {
        let mut view = DiagramView {
            pan: (10.0, 20.0),
            ..DiagramView::default()
        };
        // Diagram point under (110, 220) at zoom 1 is (100, 200).
        view.zoom_at(2.0, (110.0, 220.0));
        assert_eq!(view.zoom, 2.0);
        assert_eq!(view.pan, (-90.0, -180.0));
        view.zoom_at(10.0, (0.0, 0.0));
        assert_eq!(view.zoom, MAX_ZOOM);
        view.zoom_at(0.0001, (0.0, 0.0));
        assert_eq!(view.zoom, MIN_ZOOM);
    }

    #[test]
    fn fit_centres_and_never_magnifies() {
        let mut view = DiagramView {
            layout: Some(Arc::new(ErLayout {
                width: 100.0,
                height: 50.0,
                ..ErLayout::default()
            })),
            ..DiagramView::default()
        };
        view.fit(1000.0, 500.0);
        assert_eq!(view.zoom, 1.0);
        assert_eq!(view.pan, (450.0, 225.0));
        view.layout = Some(Arc::new(ErLayout {
            width: 4000.0,
            height: 100.0,
            ..ErLayout::default()
        }));
        view.fit(1048.0, 500.0);
        assert_eq!(view.zoom, 0.25);
    }
}
