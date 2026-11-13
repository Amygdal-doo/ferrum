//! Where the tool panes (Git, Agent, Term, CI, Logs, Debug) sit around the
//! editor: three docks — left, right and bottom — each a split of the
//! tools placed in it. A tool keeps its slot (dock, order, share) while it
//! is closed, so toggling it brings it back where it was. Stored per
//! project in the session file.

use serde::{Deserialize, Serialize};

/// A tool that opens as a pane inside the editor window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Git,
    Agent,
    Term,
    Ci,
    Logs,
    Debug,
}

impl ToolKind {
    pub const ALL: [ToolKind; 6] = [
        ToolKind::Git,
        ToolKind::Agent,
        ToolKind::Term,
        ToolKind::Ci,
        ToolKind::Logs,
        ToolKind::Debug,
    ];

    pub fn title(self) -> &'static str {
        match self {
            ToolKind::Git => "Git",
            ToolKind::Agent => "Agent",
            ToolKind::Term => "Terminal",
            ToolKind::Ci => "CI",
            ToolKind::Logs => "Logs",
            ToolKind::Debug => "Debug",
        }
    }

    /// Narrowest usable width of the pane, in pixels.
    pub fn min_width(self) -> f32 {
        match self {
            ToolKind::Agent | ToolKind::Ci => 360.,
            ToolKind::Git | ToolKind::Term | ToolKind::Logs => 320.,
            ToolKind::Debug => 260.,
        }
    }

    /// Where the tool opens the first time.
    pub fn default_dock(self) -> Dock {
        match self {
            ToolKind::Term | ToolKind::Logs => Dock::Bottom,
            _ => Dock::Right,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dock {
    Left,
    Right,
    Bottom,
}

impl Dock {
    pub const ALL: [Dock; 3] = [Dock::Left, Dock::Right, Dock::Bottom];

    pub fn title(self) -> &'static str {
        match self {
            Dock::Left => "Left",
            Dock::Right => "Right",
            Dock::Bottom => "Bottom",
        }
    }

    /// Side docks stack their tools top to bottom; the bottom dock puts
    /// them side by side.
    pub fn stacks_vertically(self) -> bool {
        self != Dock::Bottom
    }

    fn default_size(self) -> f32 {
        match self {
            Dock::Left | Dock::Right => 440.,
            Dock::Bottom => 280.,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ToolSlot {
    pub kind: ToolKind,
    #[serde(default)]
    pub open: bool,
    /// Share of the dock among its open tools (normalised when drawn).
    #[serde(default = "one")]
    pub weight: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockArea {
    #[serde(default)]
    pub slots: Vec<ToolSlot>,
    /// Width (left / right) or height (bottom) in pixels.
    #[serde(default)]
    pub size: f32,
}

/// Smallest share a pane can be dragged down to.
const MIN_SHARE: f32 = 0.1;
/// Bottom dock height limits, in pixels.
pub const MIN_BOTTOM: f32 = 120.;
/// Largest dock size, in pixels.
pub const MAX_DOCK: f32 = 1600.;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolLayout {
    pub left: DockArea,
    pub right: DockArea,
    pub bottom: DockArea,
}

impl Default for ToolLayout {
    fn default() -> Self {
        let area = |dock: Dock| DockArea {
            slots: ToolKind::ALL
                .iter()
                .filter(|k| k.default_dock() == dock)
                .map(|&kind| ToolSlot {
                    kind,
                    open: false,
                    weight: 1.0,
                })
                .collect(),
            size: dock.default_size(),
        };
        Self {
            left: area(Dock::Left),
            right: area(Dock::Right),
            bottom: area(Dock::Bottom),
        }
    }
}

impl ToolLayout {
    pub fn area(&self, dock: Dock) -> &DockArea {
        match dock {
            Dock::Left => &self.left,
            Dock::Right => &self.right,
            Dock::Bottom => &self.bottom,
        }
    }

    fn area_mut(&mut self, dock: Dock) -> &mut DockArea {
        match dock {
            Dock::Left => &mut self.left,
            Dock::Right => &mut self.right,
            Dock::Bottom => &mut self.bottom,
        }
    }

    /// Repairs a loaded layout: every tool exactly once, sane sizes.
    pub fn normalize(&mut self) {
        let mut seen = Vec::new();
        for dock in Dock::ALL {
            let area = self.area_mut(dock);
            area.slots.retain(|slot| {
                let fresh = !seen.contains(&slot.kind);
                seen.push(slot.kind);
                fresh
            });
            for slot in &mut area.slots {
                if !slot.weight.is_finite() || slot.weight <= 0. {
                    slot.weight = 1.0;
                }
            }
            if !area.size.is_finite() || area.size <= 0. {
                area.size = dock.default_size();
            }
            area.size = area.size.min(MAX_DOCK);
        }
        for kind in ToolKind::ALL {
            if !seen.contains(&kind) {
                self.area_mut(kind.default_dock()).slots.push(ToolSlot {
                    kind,
                    open: false,
                    weight: 1.0,
                });
            }
        }
    }

    /// The dock holding `kind` and its index there.
    pub fn locate(&self, kind: ToolKind) -> Option<(Dock, usize)> {
        Dock::ALL.into_iter().find_map(|dock| {
            self.area(dock)
                .slots
                .iter()
                .position(|s| s.kind == kind)
                .map(|i| (dock, i))
        })
    }

    pub fn dock_of(&self, kind: ToolKind) -> Option<Dock> {
        self.locate(kind).map(|(dock, _)| dock)
    }

    pub fn is_open(&self, kind: ToolKind) -> bool {
        self.locate(kind)
            .is_some_and(|(dock, i)| self.area(dock).slots[i].open)
    }

    pub fn open_tools(&self) -> Vec<ToolKind> {
        Dock::ALL
            .into_iter()
            .flat_map(|dock| self.visible(dock))
            .map(|(kind, _)| kind)
            .collect()
    }

    pub fn set_open(&mut self, kind: ToolKind, open: bool) {
        if self.locate(kind).is_none() {
            self.normalize();
        }
        if let Some((dock, i)) = self.locate(kind) {
            self.area_mut(dock).slots[i].open = open;
        }
    }

    /// Opens or closes `kind`; returns whether it is now open.
    pub fn toggle(&mut self, kind: ToolKind) -> bool {
        let open = !self.is_open(kind);
        self.set_open(kind, open);
        open
    }

    /// The open tools of `dock` in order, with their share (summing to 1).
    pub fn visible(&self, dock: Dock) -> Vec<(ToolKind, f32)> {
        let open: Vec<&ToolSlot> = self.area(dock).slots.iter().filter(|s| s.open).collect();
        let total: f32 = open.iter().map(|s| s.weight).sum();
        open.iter()
            .map(|s| (s.kind, if total > 0. { s.weight / total } else { 0. }))
            .collect()
    }

    /// The dock's size, at least as wide as its open tools need.
    pub fn dock_size(&self, dock: Dock) -> f32 {
        let size = self.area(dock).size;
        if dock == Dock::Bottom {
            return size.max(MIN_BOTTOM);
        }
        self.visible(dock)
            .iter()
            .map(|(kind, _)| kind.min_width())
            .fold(size, f32::max)
    }

    pub fn set_dock_size(&mut self, dock: Dock, size: f32) {
        let min = match dock {
            Dock::Bottom => MIN_BOTTOM,
            _ => self
                .visible(dock)
                .iter()
                .map(|(kind, _)| kind.min_width())
                .fold(200., f32::max),
        };
        self.area_mut(dock).size = size.clamp(min, MAX_DOCK.max(min));
    }

    /// Moves `kind` to the end of `dock` and opens it.
    pub fn move_to(&mut self, kind: ToolKind, dock: Dock) {
        let len = self.area(dock).slots.len();
        self.insert(kind, dock, len);
    }

    /// Moves `kind` right after (`after`) or before `target`, splitting
    /// `target`'s place with it, and opens it.
    pub fn move_next_to(&mut self, kind: ToolKind, target: ToolKind, after: bool) {
        if kind == target {
            return;
        }
        let Some((dock, _)) = self.locate(target) else {
            return;
        };
        self.take(kind);
        let Some((_, index)) = self.locate(target) else {
            return;
        };
        self.insert(kind, dock, index + usize::from(after));
    }

    fn take(&mut self, kind: ToolKind) -> Option<ToolSlot> {
        let (dock, i) = self.locate(kind)?;
        Some(self.area_mut(dock).slots.remove(i))
    }

    fn insert(&mut self, kind: ToolKind, dock: Dock, index: usize) {
        let mut slot = self.take(kind).unwrap_or(ToolSlot {
            kind,
            open: true,
            weight: 1.0,
        });
        // An even share of the panes already there.
        let visible = self.visible(dock);
        slot.weight = if visible.is_empty() {
            1.0
        } else {
            let open = self.area(dock).slots.iter().filter(|s| s.open);
            let (sum, n) = open.fold((0., 0.), |(sum, n), s| (sum + s.weight, n + 1.));
            sum / n
        };
        slot.open = true;
        let area = self.area_mut(dock);
        let index = index.min(area.slots.len());
        area.slots.insert(index, slot);
    }

    /// Drags the divider after the `boundary`-th open tool of `dock` to
    /// `position` (0..1 along the dock). Neighbours keep a minimum share.
    pub fn drag_divider(&mut self, dock: Dock, boundary: usize, position: f32) {
        let visible = self.visible(dock);
        if boundary + 1 >= visible.len() {
            return;
        }
        let start: f32 = visible[..boundary].iter().map(|(_, f)| f).sum();
        let pair = visible[boundary].1 + visible[boundary + 1].1;
        let first =
            (position - start).clamp(MIN_SHARE.min(pair / 2.), pair - MIN_SHARE.min(pair / 2.));
        let mut shares: Vec<f32> = visible.iter().map(|(_, f)| *f).collect();
        shares[boundary] = first;
        shares[boundary + 1] = pair - first;
        let area = self.area_mut(dock);
        let mut it = shares.into_iter();
        for slot in area.slots.iter_mut().filter(|s| s.open) {
            if let Some(share) = it.next() {
                slot.weight = share;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(layout: &ToolLayout, dock: Dock) -> Vec<ToolKind> {
        layout.visible(dock).into_iter().map(|(k, _)| k).collect()
    }

    #[test]
    fn defaults_put_term_below_and_the_rest_right() {
        let layout = ToolLayout::default();
        assert!(layout.open_tools().is_empty());
        assert_eq!(layout.dock_of(ToolKind::Git), Some(Dock::Right));
        assert_eq!(layout.dock_of(ToolKind::Agent), Some(Dock::Right));
        assert_eq!(layout.dock_of(ToolKind::Term), Some(Dock::Bottom));
    }

    #[test]
    fn all_three_open_at_once_and_toggle_back_in_place() {
        let mut layout = ToolLayout::default();
        for kind in [ToolKind::Git, ToolKind::Agent, ToolKind::Term] {
            assert!(layout.toggle(kind));
        }
        assert_eq!(
            kinds(&layout, Dock::Right),
            [ToolKind::Git, ToolKind::Agent]
        );
        assert_eq!(kinds(&layout, Dock::Bottom), [ToolKind::Term]);
        assert!(!layout.toggle(ToolKind::Git));
        assert_eq!(kinds(&layout, Dock::Right), [ToolKind::Agent]);
        assert!(layout.toggle(ToolKind::Git));
        assert_eq!(
            kinds(&layout, Dock::Right),
            [ToolKind::Git, ToolKind::Agent]
        );
    }

    #[test]
    fn moving_between_docks_and_next_to_another_tool() {
        let mut layout = ToolLayout::default();
        layout.set_open(ToolKind::Git, true);
        layout.set_open(ToolKind::Agent, true);
        layout.move_to(ToolKind::Git, Dock::Bottom);
        assert_eq!(kinds(&layout, Dock::Bottom), [ToolKind::Git]);
        assert_eq!(kinds(&layout, Dock::Right), [ToolKind::Agent]);
        layout.move_next_to(ToolKind::Term, ToolKind::Agent, false);
        assert_eq!(
            kinds(&layout, Dock::Right),
            [ToolKind::Term, ToolKind::Agent]
        );
        layout.move_to(ToolKind::Agent, Dock::Left);
        assert_eq!(kinds(&layout, Dock::Left), [ToolKind::Agent]);
        assert!(layout.is_open(ToolKind::Agent));
    }

    #[test]
    fn dividers_and_dock_sizes_respect_minimums() {
        let mut layout = ToolLayout::default();
        layout.set_open(ToolKind::Git, true);
        layout.set_open(ToolKind::Agent, true);
        layout.drag_divider(Dock::Right, 0, 0.25);
        let shares: Vec<f32> = layout.visible(Dock::Right).iter().map(|v| v.1).collect();
        assert!((shares[0] - 0.25).abs() < 1e-4 && (shares[1] - 0.75).abs() < 1e-4);
        layout.drag_divider(Dock::Right, 0, 0.0);
        assert!((layout.visible(Dock::Right)[0].1 - MIN_SHARE).abs() < 1e-4);
        layout.set_dock_size(Dock::Right, 100.);
        assert_eq!(layout.dock_size(Dock::Right), 360., "the chat needs 360 px");
        layout.set_dock_size(Dock::Bottom, 10.);
        assert_eq!(layout.dock_size(Dock::Bottom), MIN_BOTTOM);
    }

    #[test]
    fn normalize_repairs_saved_layouts() {
        let mut layout: ToolLayout = serde_json::from_str(
            r#"{"right":{"slots":[{"kind":"git","open":true},{"kind":"git"}],"size":-1}}"#,
        )
        .expect("parse");
        layout.normalize();
        let gits = layout
            .right
            .slots
            .iter()
            .filter(|s| s.kind == ToolKind::Git);
        assert_eq!(gits.count(), 1, "duplicates dropped");
        assert!(layout.is_open(ToolKind::Git));
        assert!(layout.right.size > 0.);
        for kind in ToolKind::ALL {
            assert!(layout.locate(kind).is_some(), "{kind:?}");
        }
    }
}
