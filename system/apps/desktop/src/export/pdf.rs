//! The scene as a one-page vector PDF, written by hand: no PDF crate is in
//! the workspace and the drawing needs only paths, colours and the
//! standard Helvetica fonts (nothing to embed). Text outside Windows-1252
//! is approximated (`→` as `->`).

use std::fmt::Write as _;

use super::scene::{Scene, Shape};

/// Bézier handle length for a quarter circle of radius 1.
const KAPPA: f32 = 0.552_284_8;

pub fn pdf(scene: &Scene) -> Vec<u8> {
    let h = scene.height;
    // PDF's y axis points up.
    let p = |x: f32, y: f32| format!("{x:.2} {:.2}", h - y);
    let mut ops = String::from("1 j 1 J\n");
    for shape in &scene.shapes {
        match shape {
            Shape::Rect {
                x,
                y,
                w,
                h: rh,
                radius,
                fill,
                stroke,
            } => {
                let (x, y, w, rh) = (*x, *y, *w, *rh);
                let r = radius.min(w / 2.).min(rh / 2.);
                let k = r * KAPPA;
                let _ = write!(ops, "{} m ", p(x + r, y));
                let _ = write!(ops, "{} l ", p(x + w - r, y));
                let _ = write!(
                    ops,
                    "{} {} {} c ",
                    p(x + w - r + k, y),
                    p(x + w, y + r - k),
                    p(x + w, y + r)
                );
                let _ = write!(ops, "{} l ", p(x + w, y + rh - r));
                let _ = write!(
                    ops,
                    "{} {} {} c ",
                    p(x + w, y + rh - r + k),
                    p(x + w - r + k, y + rh),
                    p(x + w - r, y + rh)
                );
                let _ = write!(ops, "{} l ", p(x + r, y + rh));
                let _ = write!(
                    ops,
                    "{} {} {} c ",
                    p(x + r - k, y + rh),
                    p(x, y + rh - r + k),
                    p(x, y + rh - r)
                );
                let _ = write!(ops, "{} l ", p(x, y + r));
                let _ = write!(
                    ops,
                    "{} {} {} c h ",
                    p(x, y + r - k),
                    p(x + r - k, y),
                    p(x + r, y)
                );
                if let Some(c) = fill {
                    let _ = write!(ops, "{} rg ", color(*c));
                }
                if let Some((c, width)) = stroke {
                    let _ = write!(ops, "{} RG {width:.2} w ", color(*c));
                }
                ops.push_str(match (fill, stroke) {
                    (Some(_), Some(_)) => "B\n",
                    (Some(_), None) => "f\n",
                    (None, Some(_)) => "S\n",
                    (None, None) => "n\n",
                });
            }
            Shape::Line {
                points,
                color: c,
                width,
                dashed,
            } => {
                let dash = if *dashed { "[5 4] 0 d" } else { "[] 0 d" };
                let _ = write!(ops, "{} RG {width:.2} w {dash} ", color(*c));
                for (i, (x, y)) in points.iter().enumerate() {
                    let _ = write!(ops, "{} {} ", p(*x, *y), if i == 0 { "m" } else { "l" });
                }
                ops.push_str("S [] 0 d\n");
            }
            Shape::Arrow {
                from,
                tip,
                color: c,
            } => {
                let [a, b, t] = super::arrow(*from, *tip);
                let _ = writeln!(
                    ops,
                    "{} rg {} m {} l {} l h f",
                    color(*c),
                    p(a.0, a.1),
                    p(b.0, b.1),
                    p(t.0, t.1)
                );
            }
            Shape::Text {
                x,
                y,
                size,
                bold,
                color: c,
                middle,
                text,
            } => {
                // Helvetica's average advance, for centring.
                let width = text.chars().count() as f32 * size * if *bold { 0.56 } else { 0.52 };
                let x = if *middle { x - width / 2. } else { *x };
                let font = if *bold { "F2" } else { "F1" };
                let _ = writeln!(
                    ops,
                    "BT /{font} {size:.2} Tf {} rg {} Td ({}) Tj ET",
                    color(*c),
                    p(x, *y),
                    winansi(text)
                );
            }
        }
    }
    document(scene.width, h, &ops)
}

fn color(c: u32) -> String {
    let part = |shift: u32| ((c >> shift) & 0xff) as f32 / 255.;
    format!("{:.3} {:.3} {:.3}", part(16), part(8), part(0))
}

/// `text` for a PDF string in WinAnsiEncoding, `(`, `)` and `\` escaped,
/// bytes above 0x7f as octal escapes so the content stays ASCII.
fn winansi(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let byte: Option<u8> = match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                Some(c as u8)
            }
            ' '..='~' => Some(c as u8),
            '…' => Some(0x85),
            '•' => Some(0x95),
            '–' => Some(0x96),
            '—' => Some(0x97),
            '→' => {
                out.push_str("->");
                None
            }
            '\u{a0}'..='\u{ff}' => Some(c as u32 as u8),
            _ => Some(b'?'),
        };
        match byte {
            Some(b) if b > 0x7e => {
                let _ = write!(out, "\\{b:03o}");
            }
            Some(b) => out.push(b as char),
            None => {}
        }
    }
    out
}

/// The file around the page's drawing `ops`.
fn document(width: f32, height: f32, ops: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width:.2} {height:.2}] \
             /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>"
        ),
        format!("<< /Length {} >>\nstream\n{ops}endstream", ops.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .to_owned(),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{body}\nendobj\n", i + 1);
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_escaped_into_windows_1252() {
        assert_eq!(winansi("a (b) \\ c"), "a \\(b\\) \\\\ c");
        assert_eq!(winansi("x → y — z · ✓"), "x -> y \\227 z \\267 ?");
    }

    #[test]
    fn the_cross_reference_table_points_at_every_object() {
        let scene = super::super::scene::build(&ferrumsystem_core::demos::banking());
        let bytes = pdf(&scene);
        let text = String::from_utf8(bytes).expect("ascii");
        assert!(text.starts_with("%PDF-1.4\n") && text.ends_with("%%EOF\n"));
        let xref: usize = text
            .rsplit("startxref\n")
            .next()
            .and_then(|t| t.lines().next())
            .and_then(|n| n.parse().ok())
            .expect("startxref");
        assert!(text[xref..].starts_with("xref\n0 7\n"));
        for (i, line) in text[xref..].lines().skip(3).take(6).enumerate() {
            let offset: usize = line[..10].parse().expect("offset");
            assert!(
                text[offset..].starts_with(&format!("{} 0 obj", i + 1)),
                "{line}"
            );
        }
        assert!(text.contains("(Nova Bank"), "the name is drawn");
    }
}
