//! SVG lane rails for one graph row (Tower style: thin rails, hollow nodes).

use super::LANE_W;
use crate::components::lane_color;
use dioxus::prelude::*;
use ferrumgit_application::GraphRow;

pub fn x(lane: usize) -> f64 {
    lane as f64 * LANE_W + LANE_W / 2.0 + 6.0
}

fn segment(from: usize, to: usize, y0: f64, y1: f64) -> String {
    let (x0, x1) = (x(from), x(to));
    if from == to {
        format!("M{x0} {y0} L{x1} {y1}")
    } else {
        let m = (y0 + y1) / 2.0;
        format!("M{x0} {y0} C{x0} {m} {x1} {m} {x1} {y1}")
    }
}

#[component]
pub fn Lanes(graph: GraphRow, lanes: usize, width: f64, height: f64, is_head: bool) -> Element {
    let mid = height / 2.0;
    let clip = |l: usize| l < lanes;
    let up: Vec<_> = graph
        .up
        .iter()
        .filter(|e| clip(e.from) && clip(e.to))
        .copied()
        .collect();
    let down: Vec<_> = graph
        .down
        .iter()
        .filter(|e| clip(e.from) && clip(e.to))
        .copied()
        .collect();
    let cx = x(graph.lane.min(lanes.saturating_sub(1)));
    let color = lane_color(graph.color);
    let fill = if is_head { color } else { "var(--background)" };
    rsx! {
        svg { width: "{width}", height: "{height}", view_box: "0 0 {width} {height}", class: "shrink-0 overflow-visible",
            for (i, e) in up.into_iter().enumerate() {
                path { key: "u{i}", d: segment(e.from, e.to, 0.0, mid), stroke: lane_color(e.color), stroke_width: "1.6", fill: "none" }
            }
            for (i, e) in down.into_iter().enumerate() {
                path { key: "d{i}", d: segment(e.from, e.to, mid, height), stroke: lane_color(e.color), stroke_width: "1.6", fill: "none" }
            }
            circle { cx: "{cx}", cy: "{mid}", r: "4.5", fill: fill, stroke: "var(--foreground)", stroke_width: "1.6" }
        }
    }
}

/// Straight rails through a month header (lanes continuing into the next commit).
#[component]
pub fn PassThrough(lanes: Vec<(usize, usize)>, width: f64, height: f64) -> Element {
    rsx! {
        svg { width: "{width}", height: "{height}", class: "shrink-0",
            for (i, (lane, color)) in lanes.into_iter().enumerate() {
                path { key: "{i}", d: format!("M{0} 0 L{0} {height}", x(lane)), stroke: lane_color(color), stroke_width: "1.6", fill: "none" }
            }
        }
    }
}
