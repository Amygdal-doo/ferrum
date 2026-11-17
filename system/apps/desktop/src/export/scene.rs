//! The diagram as shapes in diagram units (the canvas's layout, light
//! colours on white, with the name on top): the one drawing the SVG and
//! PDF writers both turn into a file.

use ferrumsystem_core::{EdgeStyle, Kind, System};

use crate::layout::{self, ITEM_H, MAX_ITEMS, NODE_H};

/// Room above the bands for the name and tagline.
const HEAD: f32 = 52.;
const TEXT: u32 = 0x111827;
const MUTED: u32 = 0x6b7280;
const WHITE: u32 = 0xffffff;

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        radius: f32,
        fill: Option<u32>,
        /// Colour and width.
        stroke: Option<(u32, f32)>,
    },
    Line {
        points: Vec<(f32, f32)>,
        color: u32,
        width: f32,
        dashed: bool,
    },
    /// A filled arrowhead at `tip`, pointing away from `from`.
    Arrow {
        from: (f32, f32),
        tip: (f32, f32),
        color: u32,
    },
    /// Text on the baseline `y`; centred on `x` when `middle`.
    Text {
        x: f32,
        y: f32,
        size: f32,
        bold: bool,
        color: u32,
        middle: bool,
        text: String,
    },
}

pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub shapes: Vec<Shape>,
}

/// `color` at `alpha` over white, as one opaque colour.
fn tint(color: u32, alpha: f32) -> u32 {
    let mix = |shift: u32| {
        let c = ((color >> shift) & 0xff) as f32;
        ((c * alpha + 255. * (1. - alpha)).round() as u32) << shift
    };
    mix(16) | mix(8) | mix(0)
}

pub fn edge_color(style: EdgeStyle) -> u32 {
    match style {
        EdgeStyle::Request => MUTED,
        EdgeStyle::Rpc => 0x38bdf8,
        EdgeStyle::Async => 0xa855f7,
        EdgeStyle::Socket => 0x22c55e,
        EdgeStyle::Cdc => 0xeab308,
    }
}

/// `text` cut to about `chars` characters.
fn fit(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        return text.to_owned();
    }
    let cut: String = text.chars().take(chars.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

fn text(x: f32, y: f32, size: f32, color: u32, text: impl Into<String>) -> Shape {
    Shape::Text {
        x,
        y,
        size,
        bold: false,
        color,
        middle: false,
        text: text.into(),
    }
}

fn bold(mut shape: Shape) -> Shape {
    if let Shape::Text { bold, .. } = &mut shape {
        *bold = true;
    }
    shape
}

fn centred(mut shape: Shape) -> Shape {
    if let Shape::Text { middle, .. } = &mut shape {
        *middle = true;
    }
    shape
}

pub fn build(system: &System) -> Scene {
    let l = layout::compute(system);
    let mut shapes = vec![Shape::Rect {
        x: 0.,
        y: 0.,
        w: l.width,
        h: l.height + HEAD,
        radius: 0.,
        fill: Some(WHITE),
        stroke: None,
    }];
    shapes.push(bold(text(24., 30., 18., TEXT, system.name.clone())));
    shapes.push(text(24., 47., 11., MUTED, fit(&system.tagline, 110)));
    let down = |y: f32| y + HEAD;

    for (i, band) in &l.bands {
        let layer = &system.layers[*i];
        shapes.push(Shape::Rect {
            x: band.x,
            y: down(band.y),
            w: band.w,
            h: band.h,
            radius: 10.,
            fill: Some(tint(layer.color, 0.06)),
            stroke: Some((tint(layer.color, 0.35), 1.)),
        });
        shapes.push(bold(text(
            band.x + 12.,
            down(band.y) + 17.,
            11.,
            layer.color,
            layer.name.clone(),
        )));
    }

    for (index, route) in layout::routes(system, &l) {
        let edge = &system.edges[index];
        let color = edge.color.unwrap_or_else(|| edge_color(edge.style));
        let points: Vec<(f32, f32)> = route.iter().map(|(x, y)| (*x, down(*y))).collect();
        if let [.., from, tip] = points.as_slice() {
            shapes.push(Shape::Arrow {
                from: *from,
                tip: *tip,
                color,
            });
        }
        shapes.push(Shape::Line {
            points,
            color,
            width: edge.width.unwrap_or(1.4),
            dashed: edge.style.dashed(),
        });
        if let Some(label) = &edge.label
            && let Some((x, y)) = layout::label_point(&route)
        {
            let label = fit(label, 22);
            let w = label.chars().count() as f32 * 5.6 + 10.;
            shapes.push(Shape::Rect {
                x: x - w / 2.,
                y: down(y) - 8.,
                w,
                h: 16.,
                radius: 3.,
                fill: Some(WHITE),
                stroke: Some((0xe5e7eb, 1.)),
            });
            shapes.push(centred(text(x, down(y) + 3.5, 9.5, MUTED, label)));
        }
    }

    for node in &system.nodes {
        let Some(r) = l.nodes.get(&node.id) else {
            continue;
        };
        let (x, y) = (r.x, down(r.y));
        let layer_color = system
            .layers
            .iter()
            .find(|l| l.id == node.layer)
            .map_or(MUTED, |l| l.color);
        let st = &node.style;
        let accent = st.color.unwrap_or(layer_color);
        shapes.push(Shape::Rect {
            x,
            y,
            w: r.w,
            h: r.h,
            radius: st.radius.unwrap_or(8.),
            fill: Some(st.fill.unwrap_or(WHITE)),
            stroke: (st.border != Some(false)).then_some((tint(accent, 0.55), 1.)),
        });
        shapes.push(Shape::Rect {
            x,
            y: y + 6.,
            w: 3.,
            h: r.h - 12.,
            radius: 1.5,
            fill: Some(accent),
            stroke: None,
        });
        let service = Kind::find(&node.kind).map(|k| k.service(system.provider));
        let tech = node
            .tech
            .as_deref()
            .and_then(|t| crate::icons::tech_info(system, t))
            .map(|(name, _, _)| name);
        let above = match (st.above.as_deref(), service, tech) {
            (Some(above), ..) if !above.is_empty() => above.to_owned(),
            (_, Some(service), Some(tech)) => format!("{service} · {tech}"),
            (_, service, tech) => service.map(str::to_owned).or(tech).unwrap_or_default(),
        };
        shapes.push(text(x + 12., y + 18., 9.5, MUTED, fit(&above, 34)));
        shapes.push(bold(text(x + 12., y + 36., 13., TEXT, fit(&node.name, 24))));
        let mut row = y + NODE_H;
        if let Some(below) = st.below.as_deref().filter(|t| !t.is_empty()) {
            shapes.push(text(x + 12., y + 51., 10.5, MUTED, fit(below, 30)));
            row += 14.;
        }
        if let Some(body) = st.body.as_deref().filter(|t| !t.is_empty()) {
            row += 8.;
            for line in wrap(body, 30).into_iter().take(6) {
                shapes.push(text(x + 12., row, 10., TEXT, line));
                row += 14.;
            }
        }
        for item in node.items.iter().take(MAX_ITEMS) {
            let line = if item.detail.is_empty() {
                format!("• {}", item.name)
            } else {
                format!("• {} — {}", item.name, item.detail)
            };
            shapes.push(text(x + 12., row + 2., 10., TEXT, fit(&line, 32)));
            row += ITEM_H;
        }
        if node.items.len() > MAX_ITEMS {
            let more = format!("+{} more", node.items.len() - MAX_ITEMS);
            shapes.push(text(x + 12., row + 2., 10., MUTED, more));
        }
    }
    Scene {
        width: l.width,
        height: l.height + HEAD,
        shapes,
    }
}

/// `text` in lines of at most about `width` characters.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_component_layer_and_connection_is_drawn() {
        for system in ferrumsystem_core::demos::all() {
            let scene = build(&system);
            let texts: Vec<&str> = scene
                .shapes
                .iter()
                .filter_map(|s| match s {
                    Shape::Text { text, .. } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            for layer in &system.layers {
                assert!(texts.contains(&layer.name.as_str()), "{}", layer.name);
            }
            for node in &system.nodes {
                assert!(
                    texts.iter().any(|t| *t == fit(&node.name, 24)),
                    "{}",
                    node.id
                );
            }
            let lines = scene
                .shapes
                .iter()
                .filter(|s| matches!(s, Shape::Line { .. }))
                .count();
            assert_eq!(lines, system.edges.len());
        }
        assert_eq!(tint(0x000000, 0.5), 0x808080);
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(fit("abcdef", 4), "abc…");
    }
}
