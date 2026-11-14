//! Where things go on the diagram, in unscaled units (the canvas applies
//! zoom and pan): one band per layer, components left to right inside it,
//! and an orthogonal route for every connection.

use std::collections::HashMap;

use ferrumsystem_core::{Node, Side, System};

pub const NODE_W: f32 = 178.;
pub const NODE_H: f32 = 58.;
/// One inner item's row on a card.
pub const ITEM_H: f32 = 17.;
/// Items shown on a card (the rest as "+N more").
pub const MAX_ITEMS: usize = 6;
pub const GAP_X: f32 = 30.;
/// Band label row.
pub const BAND_HEAD: f32 = 24.;
pub const BAND_PAD: f32 = 18.;
const BAND_GAP: f32 = 12.;
const MARGIN: f32 = 24.;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A card's height: the header plus a row per inner item.
pub fn node_height(node: &Node) -> f32 {
    let rows = node.items.len().min(MAX_ITEMS) + usize::from(node.items.len() > MAX_ITEMS);
    let below = if node.style.below.as_deref().is_some_and(|t| !t.is_empty()) {
        14.
    } else {
        0.
    };
    // About 30 characters per line of the card's paragraph.
    let body = node
        .style
        .body
        .as_deref()
        .filter(|t| !t.is_empty())
        .map(|t| 8. + (t.chars().count() as f32 / 30.).ceil().min(6.) * 14.)
        .unwrap_or(0.);
    NODE_H + below + body + rows as f32 * ITEM_H + if rows > 0 { 6. } else { 0. }
}

impl Rect {
    /// Middle of `side`.
    pub fn side(&self, side: Side) -> (f32, f32) {
        match side {
            Side::Top => (self.cx(), self.y),
            Side::Right => (self.right(), self.cy()),
            Side::Bottom => (self.cx(), self.bottom()),
            Side::Left => (self.x, self.cy()),
        }
    }

    /// The side nearest to `(x, y)`.
    pub fn nearest_side(&self, x: f32, y: f32) -> Side {
        Side::ALL
            .into_iter()
            .min_by(|a, b| {
                let d = |s: Side| {
                    let (sx, sy) = self.side(s);
                    (sx - x).powi(2) + (sy - y).powi(2)
                };
                d(*a).total_cmp(&d(*b))
            })
            .unwrap_or(Side::Top)
    }

    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h / 2.
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
}

pub struct Layout {
    /// One per layer, in order: (layer index, band).
    pub bands: Vec<(usize, Rect)>,
    pub nodes: HashMap<String, Rect>,
    pub width: f32,
    pub height: f32,
}

pub fn compute(system: &System) -> Layout {
    let widest = system
        .layers
        .iter()
        .map(|l| system.nodes_in(&l.id).len())
        .max()
        .unwrap_or(0)
        .max(4);
    let band_w = BAND_PAD * 2. + widest as f32 * NODE_W + (widest as f32 - 1.) * GAP_X;
    let mut y = MARGIN;
    let mut bands = Vec::new();
    let mut nodes = HashMap::new();
    for (i, layer) in system.layers.iter().enumerate() {
        let tallest = system
            .nodes_in(&layer.id)
            .iter()
            .map(|n| node_height(n))
            .fold(NODE_H, f32::max);
        let band_h = BAND_HEAD + tallest + BAND_PAD * 2.;
        bands.push((
            i,
            Rect {
                x: MARGIN,
                y,
                w: band_w,
                h: band_h,
            },
        ));
        for (j, node) in system.nodes_in(&layer.id).into_iter().enumerate() {
            nodes.insert(
                node.id.clone(),
                Rect {
                    x: MARGIN + BAND_PAD + j as f32 * (NODE_W + GAP_X),
                    y: y + BAND_HEAD + BAND_PAD,
                    w: NODE_W,
                    h: node_height(node),
                },
            );
        }
        y += band_h + BAND_GAP;
    }
    Layout {
        bands,
        nodes,
        width: band_w + MARGIN * 2.,
        height: y + MARGIN - BAND_GAP,
    }
}

/// Points of the route from `a` to `b`: down (or up) out of `a`, across in
/// `a`'s band padding, then into `b`; side to side within one band.
/// `lane` (0, 1, 2…) separates routes leaving the same component.
pub fn route(a: &Rect, b: &Rect, lane: usize) -> Vec<(f32, f32)> {
    let spread = (lane % 5) as f32 * 3.;
    if (a.y - b.y).abs() < 1. {
        let (from, to) = if b.x > a.x {
            ((a.right(), a.cy()), (b.x, b.cy()))
        } else {
            ((a.x, a.cy()), (b.right(), b.cy()))
        };
        return vec![from, to];
    }
    if b.y > a.y {
        let mid = a.bottom() + 8. + spread;
        vec![
            (a.cx(), a.bottom()),
            (a.cx(), mid),
            (b.cx(), mid),
            (b.cx(), b.y),
        ]
    } else {
        let mid = a.y - 8. - spread;
        vec![
            (a.cx(), a.y),
            (a.cx(), mid),
            (b.cx(), mid),
            (b.cx(), b.bottom()),
        ]
    }
}

fn out(side: Side) -> (f32, f32) {
    match side {
        Side::Top => (0., -1.),
        Side::Right => (1., 0.),
        Side::Bottom => (0., 1.),
        Side::Left => (-1., 0.),
    }
}

/// A route between given sides: out of `a`'s side, across, into `b`'s.
pub fn route_sides(a: &Rect, sa: Side, b: &Rect, sb: Side) -> Vec<(f32, f32)> {
    const STUB: f32 = 14.;
    let (p0, p4) = (a.side(sa), b.side(sb));
    let (da, db) = (out(sa), out(sb));
    let p1 = (p0.0 + da.0 * STUB, p0.1 + da.1 * STUB);
    let p3 = (p4.0 + db.0 * STUB, p4.1 + db.1 * STUB);
    let a_horizontal = da.1 == 0.;
    let b_horizontal = db.1 == 0.;
    let middle = match (a_horizontal, b_horizontal) {
        (true, false) => vec![(p3.0, p1.1)],
        (false, true) => vec![(p1.0, p3.1)],
        (true, true) => {
            let mx = (p1.0 + p3.0) / 2.;
            vec![(mx, p1.1), (mx, p3.1)]
        }
        (false, false) => {
            let my = (p1.1 + p3.1) / 2.;
            vec![(p1.0, my), (p3.0, my)]
        }
    };
    let mut points = vec![p0, p1];
    points.extend(middle);
    points.extend([p3, p4]);
    points
}

/// Distance from `(x, y)` to the polyline.
pub fn distance(points: &[(f32, f32)], x: f32, y: f32) -> f32 {
    points
        .windows(2)
        .map(|w| {
            let ((x1, y1), (x2, y2)) = (w[0], w[1]);
            let (dx, dy) = (x2 - x1, y2 - y1);
            let len2 = dx * dx + dy * dy;
            let t = if len2 == 0. {
                0.
            } else {
                (((x - x1) * dx + (y - y1) * dy) / len2).clamp(0., 1.)
            };
            let (px, py) = (x1 + t * dx, y1 + t * dy);
            ((x - px).powi(2) + (y - py).powi(2)).sqrt()
        })
        .fold(f32::MAX, f32::min)
}

/// Middle of the longest segment, for the connection's label; `None` when
/// no segment is long enough to hold one without covering a card.
pub fn label_point(points: &[(f32, f32)]) -> Option<(f32, f32)> {
    points
        .windows(2)
        .max_by(|p, q| seg_len(p).total_cmp(&seg_len(q)))
        .filter(|p| seg_len(p) >= 90.)
        .map(|p| ((p[0].0 + p[1].0) / 2., (p[0].1 + p[1].1) / 2.))
}

fn seg_len(p: &[(f32, f32)]) -> f32 {
    (p[1].0 - p[0].0).abs() + (p[1].1 - p[0].1).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_every_component_inside_its_band() {
        for system in ferrumsystem_core::demos::all() {
            let layout = compute(&system);
            assert_eq!(layout.nodes.len(), system.nodes.len());
            for node in &system.nodes {
                let rect = layout.nodes[&node.id];
                let li = system
                    .layers
                    .iter()
                    .position(|l| l.id == node.layer)
                    .expect("layer");
                let band = layout.bands[li].1;
                assert!(
                    rect.x >= band.x && rect.right() <= band.right(),
                    "{}",
                    node.id
                );
                assert!(
                    rect.y >= band.y && rect.bottom() <= band.bottom(),
                    "{}",
                    node.id
                );
            }
        }
    }

    #[test]
    fn routes_go_from_edge_to_edge() {
        let a = Rect {
            x: 0.,
            y: 0.,
            w: 100.,
            h: 50.,
        };
        let below = Rect {
            x: 200.,
            y: 150.,
            w: 100.,
            h: 50.,
        };
        let r = route(&a, &below, 0);
        assert_eq!(r.first(), Some(&(50., 50.)));
        assert_eq!(r.last(), Some(&(250., 150.)));
        let beside = Rect {
            x: 200.,
            y: 0.,
            w: 100.,
            h: 50.,
        };
        assert_eq!(route(&a, &beside, 0), vec![(100., 25.), (200., 25.)]);
        assert_eq!(label_point(&r), Some((150., 58.)));
        assert_eq!(label_point(&[(0., 0.), (40., 0.)]), None);

        let sided = route_sides(&a, Side::Right, &below, Side::Left);
        assert_eq!(sided.first(), Some(&(100., 25.)));
        assert_eq!(sided.last(), Some(&(200., 175.)));
        assert!(distance(&sided, 150., 100.) < 1.);
        assert_eq!(below.nearest_side(205., 180.), Side::Left);
    }
}
