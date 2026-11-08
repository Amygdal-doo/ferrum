//! Scene building: frames → quads (backgrounds, cursor, decorations) and
//! glyphon text buffers. Pure layout math; no GPU calls.

use crate::gpu::Quad;
use ferrumterm_protocol::{CursorShape, Frame, ProgramState, SessionInfo, WindowInfo, flags};
use glyphon::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, Style, Weight, Wrap};
use unicode_width::UnicodeWidthStr;

/// Theme (Ghostty-like dark).
pub const BG: u32 = 0x161719;
pub const FG: u32 = 0xd8d8d8;
pub const BAR_BG: u32 = 0x1f2023;
pub const TAB_ACTIVE: u32 = 0x2a2d33;
pub const DIVIDER: u32 = 0x34373d;
pub const ACCENT: u32 = 0x5a8dd6;
pub const MUTED: u32 = 0x8a8f98;

pub fn rgb(c: u32) -> Color {
    Color::rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellMetrics {
    pub font_px: f32,
    pub cell_w: f32,
    pub cell_h: f32,
    pub tab_h: f32,
    pub pad: f32,
}

impl CellMetrics {
    pub fn measure(fs: &mut FontSystem, font_px: f32, scale: f32) -> Self {
        let line_h = (font_px * 1.25).round();
        let mut b = Buffer::new(fs, Metrics::new(font_px, line_h));
        b.set_size(fs, Some(10_000.0), Some(line_h * 2.0));
        b.set_text(
            fs,
            &"M".repeat(20),
            &Attrs::new().family(Family::Monospace),
            Shaping::Advanced,
            None,
        );
        b.shape_until_scroll(fs, false);
        let w = b
            .layout_runs()
            .next()
            .map_or(font_px * 0.6 * 20.0, |r| r.line_w);
        Self {
            font_px,
            cell_w: (w / 20.0).max(1.0),
            cell_h: line_h,
            tab_h: (34.0 * scale).round(),
            pad: (6.0 * scale).round(),
        }
    }

    /// Cells that fit in a `w`×`h` px window (below the tab bar).
    pub fn grid(&self, w: f32, h: f32) -> (u16, u16) {
        let cols = ((w - 2.0 * self.pad) / self.cell_w).floor().max(4.0);
        let rows = ((h - self.tab_h - 2.0 * self.pad) / self.cell_h)
            .floor()
            .max(2.0);
        (cols as u16, rows as u16)
    }

    pub fn origin(&self) -> (f32, f32) {
        (self.pad, self.tab_h + self.pad)
    }
}

/// One shaped text buffer for a pane's frame.
pub fn pane_text(fs: &mut FontSystem, frame: &Frame, m: &CellMetrics) -> Buffer {
    let mut b = Buffer::new(fs, Metrics::new(m.font_px, m.cell_h));
    b.set_size(
        fs,
        Some(f32::from(frame.cols) * m.cell_w + m.cell_w),
        Some(f32::from(frame.rows) * m.cell_h),
    );
    b.set_wrap(fs, Wrap::None);
    b.set_monospace_width(fs, Some(m.cell_w));
    let base = Attrs::new().family(Family::Monospace).color(rgb(FG));
    let mut spans: Vec<(&str, Attrs<'_>)> = Vec::new();
    for (i, line) in frame.lines.iter().enumerate() {
        if i > 0 {
            spans.push(("\n", base.clone()));
        }
        for s in line {
            let mut a = base.clone().color(match s.fg {
                Some(c) => rgb(c),
                None => rgb(FG),
            });
            if s.f & flags::BOLD != 0 {
                a = a.weight(Weight::BOLD);
            }
            if s.f & flags::ITALIC != 0 {
                a = a.style(Style::Italic);
            }
            if s.f & flags::DIM != 0 {
                let c = s.fg.unwrap_or(FG);
                a = a.color(Color::rgba((c >> 16) as u8, (c >> 8) as u8, c as u8, 150));
            }
            spans.push((s.t.as_str(), a));
        }
    }
    // Keep empty trailing rows so line positions stay fixed.
    for _ in frame.lines.len()..usize::from(frame.rows) {
        spans.push(("\n", base.clone()));
    }
    b.set_rich_text(fs, spans, &base, Shaping::Advanced, None);
    b.shape_until_scroll(fs, false);
    b
}

/// Backgrounds, underline/strike lines and the cursor for a pane at `(x, y)` px.
pub fn pane_quads(
    frame: &Frame,
    x: f32,
    y: f32,
    m: &CellMetrics,
    focused: bool,
    out: &mut Vec<Quad>,
) {
    let thin = (m.cell_h / 14.0).max(1.0);
    for (row, line) in frame.lines.iter().enumerate() {
        let top = y + row as f32 * m.cell_h;
        let mut col = 0usize;
        for s in line {
            let width = UnicodeWidthStr::width(s.t.as_str());
            let left = x + col as f32 * m.cell_w;
            let w = width as f32 * m.cell_w;
            if let Some(bg) = s.bg {
                out.push(Quad {
                    x: left,
                    y: top,
                    w,
                    h: m.cell_h,
                    rgb: bg,
                    alpha: 1.0,
                });
            }
            let fg = s.fg.unwrap_or(FG);
            if s.f & flags::UNDERLINE != 0 {
                out.push(Quad {
                    x: left,
                    y: top + m.cell_h - thin * 2.0,
                    w,
                    h: thin,
                    rgb: fg,
                    alpha: 1.0,
                });
            }
            if s.f & flags::STRIKE != 0 {
                out.push(Quad {
                    x: left,
                    y: top + m.cell_h / 2.0,
                    w,
                    h: thin,
                    rgb: fg,
                    alpha: 1.0,
                });
            }
            col += width;
        }
    }
    if let Some(c) = frame.cursor {
        let (cx, cy) = (
            x + f32::from(c.col) * m.cell_w,
            y + f32::from(c.row) * m.cell_h,
        );
        let beam = (m.cell_w / 6.0).max(2.0);
        if !focused {
            // Hollow box when the pane is not focused.
            for q in [
                (cx, cy, m.cell_w, thin),
                (cx, cy + m.cell_h - thin, m.cell_w, thin),
                (cx, cy, thin, m.cell_h),
                (cx + m.cell_w - thin, cy, thin, m.cell_h),
            ] {
                out.push(Quad {
                    x: q.0,
                    y: q.1,
                    w: q.2,
                    h: q.3,
                    rgb: MUTED,
                    alpha: 0.9,
                });
            }
            return;
        }
        let q = match c.shape {
            CursorShape::Block => Quad {
                x: cx,
                y: cy,
                w: m.cell_w,
                h: m.cell_h,
                rgb: FG,
                alpha: 0.55,
            },
            CursorShape::Beam => Quad {
                x: cx,
                y: cy,
                w: beam,
                h: m.cell_h,
                rgb: FG,
                alpha: 1.0,
            },
            CursorShape::Underline => Quad {
                x: cx,
                y: cy + m.cell_h - beam,
                w: m.cell_w,
                h: beam,
                rgb: FG,
                alpha: 1.0,
            },
        };
        out.push(q);
    }
}

/// A clickable tab-bar element.
#[derive(Debug, Clone)]
pub struct TabHit {
    pub x: f32,
    pub w: f32,
    pub kind: TabKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TabKind {
    Session,
    Window(String),
    NewWindow,
}

/// Label for a tab: window name, else the focused block's title / command.
fn tab_label(w: &WindowInfo) -> String {
    let b = w
        .blocks
        .iter()
        .find(|b| b.id == w.focused)
        .or(w.blocks.first());
    let title = b.map(|b| {
        b.label
            .clone()
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| {
                if b.title.is_empty() {
                    b.command.first().map_or_else(
                        || "shell".into(),
                        |c| c.rsplit('/').next().unwrap_or(c).to_owned(),
                    )
                } else {
                    b.title.clone()
                }
            })
    });
    let name = if w.name.chars().all(|c| c.is_ascii_digit()) {
        title.unwrap_or_default()
    } else {
        w.name.clone()
    };
    let status = w
        .blocks
        .iter()
        .filter_map(|b| b.status.as_ref().map(|s| s.state))
        .max_by_key(|s| match s {
            ProgramState::Blocked => 4,
            ProgramState::Error => 3,
            ProgramState::Working => 2,
            ProgramState::Done => 1,
            ProgramState::Idle => 0,
        });
    let mut label: String = name.chars().take(28).collect();
    if let Some(s) = status {
        label = format!("{} {label}", s.glyph());
    }
    label
}

/// Tab bar quads + text buffers + hit areas.
pub fn tab_bar(
    fs: &mut FontSystem,
    session: &SessionInfo,
    m: &CellMetrics,
    width: f32,
    scale: f32,
    quads: &mut Vec<Quad>,
) -> (Vec<(Buffer, f32, Color)>, Vec<TabHit>) {
    quads.push(Quad {
        x: 0.0,
        y: 0.0,
        w: width,
        h: m.tab_h,
        rgb: BAR_BG,
        alpha: 1.0,
    });
    quads.push(Quad {
        x: 0.0,
        y: m.tab_h - 1.0,
        w: width,
        h: 1.0,
        rgb: DIVIDER,
        alpha: 1.0,
    });
    let ui_px = (13.0 * scale).round();
    let line = (ui_px * 1.3).round();
    let mut texts = Vec::new();
    let mut hits = Vec::new();
    // Leave room for the macOS traffic lights.
    let mut x = (78.0 * scale).round();
    let mut item = |fs: &mut FontSystem,
                    text: &str,
                    color: u32,
                    min_w: f32,
                    x: &mut f32,
                    active: bool,
                    kind: TabKind| {
        let mut b = Buffer::new(fs, Metrics::new(ui_px, line));
        b.set_size(fs, Some(400.0 * scale), Some(line));
        b.set_text(
            fs,
            text,
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            None,
        );
        b.shape_until_scroll(fs, false);
        let text_w = b.layout_runs().next().map_or(0.0, |r| r.line_w);
        let w = (text_w + 28.0 * scale).max(min_w);
        if active {
            quads.push(Quad {
                x: *x,
                y: 4.0 * scale,
                w,
                h: m.tab_h - 4.0 * scale,
                rgb: TAB_ACTIVE,
                alpha: 1.0,
            });
            quads.push(Quad {
                x: *x,
                y: m.tab_h - 2.0 * scale,
                w,
                h: 2.0 * scale,
                rgb: ACCENT,
                alpha: 1.0,
            });
        }
        texts.push((b, *x + (w - text_w) / 2.0, rgb(color)));
        hits.push(TabHit { x: *x, w, kind });
        *x += w + 2.0 * scale;
    };
    item(
        fs,
        &format!("◆ {}", session.name),
        FG,
        0.0,
        &mut x,
        false,
        TabKind::Session,
    );
    x += 6.0 * scale;
    for w in &session.windows {
        let active = w.id == session.active_window;
        item(
            fs,
            &tab_label(w),
            if active { FG } else { MUTED },
            110.0 * scale,
            &mut x,
            active,
            TabKind::Window(w.id.to_string()),
        );
    }
    item(
        fs,
        "+",
        MUTED,
        32.0 * scale,
        &mut x,
        false,
        TabKind::NewWindow,
    );
    (texts, hits)
}

#[cfg(test)]
mod fonts {
    use super::*;

    #[test]
    #[ignore = "depends on installed system fonts"]
    fn fallback_glyphs_have_finite_metrics() {
        let mut fs = FontSystem::new();
        crate::gpu::pick_monospace(&mut fs);
        for t in ["abc", "日本語", "✓", "😀", "→", "─│", "⚠"] {
            let mut b = Buffer::new(&mut fs, Metrics::new(28.0, 35.0));
            b.set_size(&mut fs, Some(2000.0), Some(100.0));
            b.set_text(
                &mut fs,
                t,
                &Attrs::new().family(Family::Monospace),
                Shaping::Advanced,
                None,
            );
            b.shape_until_scroll(&mut fs, false);
            for r in b.layout_runs() {
                assert!(r.line_y.is_finite(), "{t:?}");
                assert!(
                    r.glyphs.iter().all(|g| g.w.is_finite() && g.x.is_finite()),
                    "{t:?}"
                );
            }
        }
    }
}
