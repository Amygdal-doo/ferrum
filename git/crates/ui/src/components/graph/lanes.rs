//! Lane rails for one graph row (Tower style: thin rails, hollow nodes),
//! painted directly with GPUI paths.

use super::LANE_W;
use crate::components::lane_color;
use ferrumgit_application::GraphRow;
use gpui::{Bounds, Hsla, PathBuilder, Pixels, Window, point, px, quad, size};

const STROKE: f32 = 1.6;
const NODE_RADIUS: f32 = 4.5;

pub fn x(lane: usize) -> f32 {
    lane as f32 * LANE_W + LANE_W / 2.0 + 6.0
}

fn stroke(
    window: &mut Window,
    origin: gpui::Point<Pixels>,
    from: usize,
    to: usize,
    y0: f32,
    y1: f32,
    color: Hsla,
) {
    let (x0, x1) = (x(from), x(to));
    let at = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
    let mut builder = PathBuilder::stroke(px(STROKE));
    builder.move_to(at(x0, y0));
    if from == to {
        builder.line_to(at(x1, y1));
    } else {
        let mid = (y0 + y1) / 2.0;
        builder.cubic_bezier_to(at(x1, y1), at(x0, mid), at(x1, mid));
    }
    match builder.build() {
        Ok(path) => window.paint_path(path, color),
        Err(error) => tracing::debug!(error = ?error, "could not build lane path"),
    }
}

/// Paints the rails and node of `graph` into `bounds`.
pub fn paint_row(
    bounds: Bounds<Pixels>,
    graph: &GraphRow,
    lanes: usize,
    is_head: bool,
    foreground: Hsla,
    background: Hsla,
    window: &mut Window,
) {
    let height = f32::from(bounds.size.height);
    let mid = height / 2.0;
    let clip = |l: usize| l < lanes;
    for e in graph.up.iter().filter(|e| clip(e.from) && clip(e.to)) {
        stroke(
            window,
            bounds.origin,
            e.from,
            e.to,
            0.0,
            mid,
            lane_color(e.color),
        );
    }
    for e in graph.down.iter().filter(|e| clip(e.from) && clip(e.to)) {
        stroke(
            window,
            bounds.origin,
            e.from,
            e.to,
            mid,
            height,
            lane_color(e.color),
        );
    }
    let cx = x(graph.lane.min(lanes.saturating_sub(1)));
    let color = lane_color(graph.color);
    let node = Bounds::new(
        point(
            bounds.origin.x + px(cx - NODE_RADIUS),
            bounds.origin.y + px(mid - NODE_RADIUS),
        ),
        size(px(NODE_RADIUS * 2.0), px(NODE_RADIUS * 2.0)),
    );
    window.paint_quad(quad(
        node,
        px(NODE_RADIUS),
        if is_head { color } else { background },
        px(STROKE),
        foreground,
        gpui::BorderStyle::Solid,
    ));
}

/// Straight rails through a month header (lanes continuing into the next commit).
pub fn paint_pass_through(bounds: Bounds<Pixels>, rails: &[(usize, usize)], window: &mut Window) {
    let height = f32::from(bounds.size.height);
    for (lane, color) in rails {
        stroke(
            window,
            bounds.origin,
            *lane,
            *lane,
            0.0,
            height,
            lane_color(*color),
        );
    }
}
