//! The scene as SVG, and that SVG rendered to PNG (resvg, at twice the
//! size, with the system's fonts).

use std::fmt::Write as _;

use super::scene::{Scene, Shape};

const FONT: &str = "Helvetica, Arial, sans-serif";

pub fn svg(scene: &Scene) -> String {
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\" font-family=\"{FONT}\">\n",
        w = scene.width,
        h = scene.height
    );
    for shape in &scene.shapes {
        match shape {
            Shape::Rect {
                x,
                y,
                w,
                h,
                radius,
                fill,
                stroke,
            } => {
                let fill = fill.map_or("none".into(), hex);
                let stroke = stroke.map_or(String::new(), |(c, width)| {
                    format!(" stroke=\"{}\" stroke-width=\"{width}\"", hex(c))
                });
                let _ = writeln!(
                    out,
                    "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" rx=\"{radius}\" fill=\"{fill}\"{stroke}/>"
                );
            }
            Shape::Line {
                points,
                color,
                width,
                dashed,
            } => {
                let points: Vec<String> = points.iter().map(|(x, y)| format!("{x},{y}")).collect();
                let dash = if *dashed {
                    " stroke-dasharray=\"5 4\""
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width}\" stroke-linejoin=\"round\"{dash}/>",
                    points.join(" "),
                    hex(*color)
                );
            }
            Shape::Arrow { from, tip, color } => {
                let [a, b, c] = super::arrow(*from, *tip);
                let _ = writeln!(
                    out,
                    "<polygon points=\"{},{} {},{} {},{}\" fill=\"{}\"/>",
                    a.0,
                    a.1,
                    b.0,
                    b.1,
                    c.0,
                    c.1,
                    hex(*color)
                );
            }
            Shape::Text {
                x,
                y,
                size,
                bold,
                color,
                middle,
                text,
            } => {
                let weight = if *bold { " font-weight=\"600\"" } else { "" };
                let anchor = if *middle {
                    " text-anchor=\"middle\""
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "<text x=\"{x}\" y=\"{y}\" font-size=\"{size}\" fill=\"{}\"{weight}{anchor}>{}</text>",
                    hex(*color),
                    escape(text)
                );
            }
        }
    }
    out.push_str("</svg>\n");
    out
}

fn hex(color: u32) -> String {
    format!("#{color:06x}")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `svg` rendered at `scale` times its size, as PNG bytes.
pub fn png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(svg, &options).map_err(|e| e.to_string())?;
    let size = tree.size();
    let (w, h) = (
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or("The diagram is too large")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| e.to_string())
}
