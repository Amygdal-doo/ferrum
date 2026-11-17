//! Export the diagram shown to PNG, SVG or PDF (File › Export, or the
//! toolbar's Export button): one drawing (see [`scene`]) written as SVG,
//! rendered from that to PNG with resvg, or written as a vector PDF.

mod pdf;
pub(crate) mod scene;
mod svg;

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrumsystem_core::System;

use crate::app::FerrumSystem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Svg,
    Pdf,
}

impl Format {
    pub const ALL: [Self; 3] = [Self::Png, Self::Svg, Self::Pdf];

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Svg => "svg",
            Self::Pdf => "pdf",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG image",
            Self::Svg => "SVG drawing",
            Self::Pdf => "PDF document",
        }
    }
}

/// `system` drawn as a file in `format`.
pub fn render(system: &System, format: Format) -> Result<Vec<u8>, String> {
    let scene = scene::build(system);
    match format {
        Format::Svg => Ok(svg::svg(&scene).into_bytes()),
        Format::Png => svg::png(&svg::svg(&scene), 2.),
        Format::Pdf => Ok(pdf::pdf(&scene)),
    }
}

/// The three corners of an arrowhead at `tip`, coming from `from`.
fn arrow(from: (f32, f32), tip: (f32, f32)) -> [(f32, f32); 3] {
    const LEN: f32 = 7.;
    const HALF: f32 = 3.5;
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
    let (ux, uy) = (dx / len, dy / len);
    let base = (tip.0 - ux * LEN, tip.1 - uy * LEN);
    [
        (base.0 - uy * HALF, base.1 + ux * HALF),
        (base.0 + uy * HALF, base.1 - ux * HALF),
        tip,
    ]
}

/// A file name from the system's name (`Nova Bank` → `nova-bank.png`).
fn file_name(system: &System, format: Format) -> String {
    let stem: String = system
        .name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let stem = stem
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let stem = if stem.is_empty() {
        "diagram".into()
    } else {
        stem
    };
    format!("{stem}.{}", format.extension())
}

impl FerrumSystem {
    /// Asks where to save the diagram as `format`, then writes it there.
    pub fn export(&mut self, format: Format, cx: &mut Context<Self>) {
        if self.is_empty() {
            return;
        }
        self.dialog = None;
        let dir = self
            .project
            .clone()
            .or_else(ferrum_ui::project::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let receiver = cx.prompt_for_new_path(&dir, Some(&file_name(&self.system, format)));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            this.update(cx, |this, cx| this.export_to(&path, format, cx))
                .ok();
        })
        .detach();
        cx.notify();
    }

    /// Draws the diagram as `format` into `path` (in the background).
    pub(crate) fn export_to(&mut self, path: &Path, format: Format, cx: &mut Context<Self>) {
        let system = self.system.clone();
        let path = path.to_path_buf();
        let task = cx.background_executor().spawn(async move {
            let bytes = render(&system, format)?;
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            Ok::<_, String>(path)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(path) => this.say(Notice::info(format!("Exported {}", path.display())), cx),
                Err(e) => this.say(Notice::error(format!("Could not export: {e}")), cx),
            })
            .ok();
        })
        .detach();
    }

    /// The Export dialog: a button per format.
    pub(crate) fn export_dialog(&self, cx: &mut Context<Self>) -> Modal {
        Modal::new("Export the diagram").child(
            v_flex()
                .gap_2()
                .child(crate::panel::para(
                    "The whole diagram as it is laid out, on a white background.",
                    cx,
                ))
                .child(h_flex().gap_2().children(Format::ALL.map(|format| {
                    Button::new(SharedString::from(format.extension()), format.label())
                        .style(ButtonStyle::Outlined)
                        .on_click(cx.listener(move |this, _, _, cx| this.export(format, cx)))
                }))),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{Fixture, draw};

    #[test]
    fn writes_svg_png_and_pdf() {
        let system = ferrumsystem_core::demos::banking();
        let svg = String::from_utf8(render(&system, Format::Svg).expect("svg")).expect("utf8");
        assert!(svg.starts_with("<svg") && svg.contains(&system.nodes[0].name));
        let png = render(&system, Format::Png).expect("png");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let width = u32::from_be_bytes(png[16..20].try_into().expect("width"));
        let scene = scene::build(&system);
        assert_eq!(width, (scene.width * 2.).ceil() as u32, "twice the size");
        assert!(
            render(&system, Format::Pdf)
                .expect("pdf")
                .starts_with(b"%PDF")
        );
        assert_eq!(file_name(&system, Format::Pdf), "nova-bank.pdf");
        let [a, b, tip] = arrow((0., 0.), (10., 0.));
        assert_eq!((a, b, tip), ((3., 3.5), (3., -3.5), (10., 0.)));
    }

    #[gpui::test]
    fn the_shown_diagram_is_exported_to_the_chosen_file(cx: &mut gpui::TestAppContext) {
        let f = Fixture::new();
        let project = f.project("shop", true);
        let (app, cx) = f.open(cx, Some(project.clone()));
        let out = project.join("shop.svg");
        app.update(cx, |app, cx| app.export_to(&out, Format::Svg, cx));
        draw(cx);
        let svg = std::fs::read_to_string(&out).expect("exported");
        assert!(svg.contains("API service"), "{svg}");
        app.read_with(cx, |app, _| {
            assert!(app.notice.is_some());
        });
    }
}
