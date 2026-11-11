//! The editor surface element. Prepaint shapes only the visible display
//! rows and builds every quad (current line, matches, selections, guides,
//! cursors, gutter); paint draws them and registers the IME input handler.

use ferrumeditor_core::display::{ExpandedLine, visual_col};
use gpui::{
    App, Bounds, ContentMask, ElementId, ElementInputHandler, Entity, Font, GlobalElementId, Hsla,
    InspectorElementId, LayoutId, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style,
    TextAlign, TextRun, Window, fill, point, prelude::*, px, relative, size,
};

use super::EditorView;
use super::layout::{LayoutSnapshot, RowLayout, RunInput, build_runs};
use crate::settings::Settings;
use crate::theme_apply::ui;

const SCROLLBAR_WIDTH: Pixels = px(10.);

pub struct EditorElement {
    editor: Entity<EditorView>,
}

impl EditorElement {
    pub fn new(editor: Entity<EditorView>) -> Self {
        Self { editor }
    }
}

/// Everything prepaint computed for paint.
pub struct Frame {
    pub(super) layout: LayoutSnapshot,
    pub(super) gutter: Bounds<Pixels>,
    pub(super) text_area: Bounds<Pixels>,
    colors: FrameColors,
    pub(super) backgrounds: Vec<PaintQuad>,
    pub(super) foreground: Vec<PaintQuad>,
    pub(super) cursors: Vec<PaintQuad>,
    pub(super) numbers: Vec<(ShapedLine, Point<Pixels>)>,
    pub(super) gutter_marks: Vec<PaintQuad>,
    scrollbar: Option<PaintQuad>,
}

#[derive(Clone, Copy)]
struct FrameColors {
    editor: Hsla,
    gutter: Hsla,
}

/// Font metrics and settings resolved once per frame.
pub(super) struct FrameStyle {
    pub(super) font: Font,
    pub(super) font_size: Pixels,
    line_height: Pixels,
    char_width: Pixels,
    scroll_past_end: bool,
    pub(super) highlight_current_line: bool,
}

impl IntoElement for EditorElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Frame {
        let settings = cx.global::<Settings>();
        let font = gpui::font(SharedString::from(settings.config.font_family.clone()));
        let font_size = px(settings.font_size());
        let line_height = (font_size * settings.config.line_height).round();
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        let char_width = text_system
            .advance(font_id, font_size, 'm')
            .map_or(font_size * 0.6, |advance| advance.width);
        let style = FrameStyle {
            font,
            font_size,
            line_height,
            char_width,
            scroll_past_end: settings.config.scroll_past_end,
            highlight_current_line: settings.config.highlight_current_line,
        };
        self.editor.update(cx, |editor, cx| {
            editor.prepaint_frame(bounds, &style, window, cx)
        })
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        window.paint_quad(fill(frame.gutter, frame.colors.gutter));
        window.paint_quad(fill(frame.text_area, frame.colors.editor));
        let line_height = frame.layout.line_height;
        window.with_content_mask(
            Some(ContentMask {
                bounds: frame.text_area,
            }),
            |window| {
                for quad in frame.backgrounds.drain(..) {
                    window.paint_quad(quad);
                }
                let origin_x = frame.layout.text_left - frame.layout.scroll_x;
                for row in &frame.layout.rows {
                    if let Err(error) = row.shaped.paint(
                        point(origin_x, row.top),
                        line_height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    ) {
                        eprintln!("ferrumeditor: failed to paint a line: {error:#}");
                    }
                }
                for quad in frame.foreground.drain(..) {
                    window.paint_quad(quad);
                }
                if focus_handle.is_focused(window) {
                    for quad in frame.cursors.drain(..) {
                        window.paint_quad(quad);
                    }
                }
            },
        );
        window.with_content_mask(
            Some(ContentMask {
                bounds: frame.gutter,
            }),
            |window| {
                for (number, origin) in &frame.numbers {
                    if let Err(error) =
                        number.paint(*origin, line_height, TextAlign::Left, None, window, cx)
                    {
                        eprintln!("ferrumeditor: failed to paint a line number: {error:#}");
                    }
                }
                for quad in frame.gutter_marks.drain(..) {
                    window.paint_quad(quad);
                }
            },
        );
        if let Some(scrollbar) = frame.scrollbar.take() {
            window.paint_quad(scrollbar);
        }
    }
}

impl EditorView {
    fn prepaint_frame(
        &mut self,
        bounds: Bounds<Pixels>,
        style: &FrameStyle,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Frame {
        let settings = cx.global::<Settings>();
        let theme = settings.theme.clone();
        let tab_size = self.state.settings.tab_size;
        let line_height = style.line_height;
        let char_width = style.char_width;
        let line_count = self.state.buffer().line_count();
        let digits = line_count.to_string().len().max(2) as f32;
        let gutter_width = if self.show_line_numbers {
            char_width * (digits + 1.) + px(16.)
        } else {
            px(14.)
        };
        let text_left = bounds.left() + gutter_width + px(6.);
        let text_width =
            (bounds.size.width - gutter_width - px(6.) - SCROLLBAR_WIDTH).max(char_width * 10.);
        let wrap = self
            .soft_wrap
            .then(|| ((text_width / char_width).floor() as usize).max(16));
        self.display_map.sync(self.state.buffer(), tab_size, wrap);

        // Vertical scroll: keep the primary cursor in view after commands.
        let total_rows = self.display_map.row_count();
        let viewport = bounds.size.height;
        let head = self.state.primary().head;
        if self.autoscroll {
            let row = self.display_map.to_display(self.state.buffer(), head).row;
            let top = line_height * row as f32;
            let margin = (line_height * 3.).min(viewport / 4.);
            if top < self.scroll_y + margin {
                self.scroll_y = top - margin;
            } else if top + line_height > self.scroll_y + viewport - margin {
                self.scroll_y = top + line_height - viewport + margin;
            }
        }
        let max_scroll = if style.scroll_past_end {
            line_height * total_rows.saturating_sub(1) as f32
        } else {
            (line_height * total_rows as f32 - viewport).max(px(0.))
        };
        self.scroll_y = self.scroll_y.clamp(px(0.), max_scroll.max(px(0.)));
        let first_row = (self.scroll_y / line_height).floor() as usize;
        let visible = (viewport / line_height).ceil() as usize + 1;
        let last_row = (first_row + visible).min(total_rows);

        let text_color = ui(&theme, "text");
        let highlights = self.highlights.clone();
        let squiggles: Vec<(std::ops::Range<usize>, Hsla)> = self
            .lsp
            .diagnostics
            .iter()
            .filter(|d| d.severity != ferrumeditor_core::lsp::Severity::Hint)
            .rev()
            .map(|d| {
                let token = super::popups::severity_token(d.severity);
                (d.range.clone(), ui(&theme, token))
            })
            .collect();
        let rows = self.shape_rows(
            first_row..last_row,
            bounds,
            style,
            &theme,
            (text_color, &squiggles),
            &highlights,
            window,
        );

        // Horizontal scroll (only without soft wrap).
        if wrap.is_none() {
            let widest = self.widest_line_chars(&rows);
            let mut max_x = (char_width * (widest as f32 + 4.) - text_width).max(px(0.));
            if self.autoscroll
                && let Some(row) = rows.iter().find(|r| r.contains_offset(head))
            {
                let x = row.x_for_offset(head);
                if x < self.scroll_x + char_width * 2. {
                    self.scroll_x = x - char_width * 4.;
                } else if x > self.scroll_x + text_width - char_width * 3. {
                    self.scroll_x = x - text_width + char_width * 4.;
                }
                max_x = max_x.max(self.scroll_x);
            }
            self.scroll_x = self.scroll_x.clamp(px(0.), max_x);
        } else {
            self.scroll_x = px(0.);
        }
        self.autoscroll = false;

        let layout = LayoutSnapshot {
            bounds,
            text_left,
            gutter_width,
            line_height,
            char_width,
            scroll_y: self.scroll_y,
            scroll_x: self.scroll_x,
            first_row,
            rows,
        };
        let mut frame = Frame {
            gutter: Bounds::new(bounds.origin, size(gutter_width, bounds.size.height)),
            text_area: Bounds::new(
                point(bounds.left() + gutter_width, bounds.top()),
                size(bounds.size.width - gutter_width, bounds.size.height),
            ),
            colors: FrameColors {
                editor: ui(&theme, "editor"),
                gutter: ui(&theme, "gutter"),
            },
            backgrounds: Vec::new(),
            foreground: Vec::new(),
            cursors: Vec::new(),
            numbers: Vec::new(),
            gutter_marks: Vec::new(),
            scrollbar: None,
            layout,
        };
        self.decorate(&mut frame, style, &theme);
        self.gutter(&mut frame, style, &theme, window);
        if line_height * total_rows as f32 > viewport && max_scroll > px(0.) {
            let content = line_height * total_rows as f32
                + if style.scroll_past_end {
                    viewport - line_height
                } else {
                    px(0.)
                };
            let thumb = (viewport * (viewport / content)).max(px(24.));
            let y = bounds.top() + (viewport - thumb) * (self.scroll_y / max_scroll);
            frame.scrollbar = Some(
                fill(
                    Bounds::new(
                        point(bounds.right() - SCROLLBAR_WIDTH + px(2.), y),
                        size(px(6.), thumb),
                    ),
                    ui(&theme, "scrollbar"),
                )
                .corner_radii(px(3.)),
            );
        }
        self.last_layout = Some(frame.layout.clone());
        frame
    }

    #[allow(clippy::too_many_arguments)]
    fn shape_rows(
        &self,
        rows: std::ops::Range<usize>,
        bounds: Bounds<Pixels>,
        style: &FrameStyle,
        theme: &ferrumeditor_core::theme::Theme,
        (text_color, squiggles): (Hsla, &[(std::ops::Range<usize>, Hsla)]),
        highlights: &[ferrumeditor_core::highlight::HighlightSpan],
        window: &mut Window,
    ) -> Vec<RowLayout> {
        let buffer = self.state.buffer();
        let tab_size = self.state.settings.tab_size;
        let mut out = Vec::with_capacity(rows.len());
        let mut cached: Option<(usize, ExpandedLine)> = None;
        for display_row in rows {
            let (buffer_row, segment) = self.display_map.buffer_segment(buffer, display_row);
            if cached.as_ref().is_none_or(|(row, _)| *row != buffer_row) {
                cached = Some((
                    buffer_row,
                    ExpandedLine::new(&buffer.line(buffer_row), tab_size),
                ));
            }
            let Some((_, expanded)) = cached.as_ref() else {
                continue;
            };
            let line_start = buffer.line_start(buffer_row);
            let line_len = buffer.line_len(buffer_row);
            let display_start = expanded.to_display(segment.start);
            let display_end = expanded.to_display(segment.end).max(display_start);
            let text: SharedString = expanded.text[display_start..display_end].to_string().into();
            let marked = self.marked_range.clone().filter(|m| {
                m.start >= line_start + segment.start && m.end <= line_start + segment.end
            });
            let runs: Vec<TextRun> = build_runs(
                &RunInput {
                    theme,
                    font: &style.font,
                    text_color,
                    highlights,
                    row_start: line_start + segment.start,
                    row_end: line_start + segment.end,
                    line_start,
                    expanded,
                    display_start,
                    marked,
                    squiggles,
                },
                text.len(),
            );
            let shaped = window
                .text_system()
                .shape_line(text, style.font_size, &runs, None);
            out.push(RowLayout {
                display_row,
                buffer_row,
                line_start,
                last_segment: segment.end >= line_len,
                segment,
                expanded: expanded.clone(),
                display_start,
                shaped,
                top: bounds.top() + style.line_height * display_row as f32 - self.scroll_y,
            });
        }
        out
    }

    /// Widest line in cells, for the horizontal scroll range. Buffers over
    /// 1 MB only grow the estimate from rows that were on screen.
    fn widest_line_chars(&mut self, rows: &[RowLayout]) -> usize {
        let version = self.state.buffer().version();
        if self.state.buffer().len() > 1024 * 1024 {
            let tab = self.state.settings.tab_size;
            let visible = rows
                .iter()
                .map(|r| visual_col(&r.expanded.text, r.expanded.text.len(), tab))
                .max()
                .unwrap_or(0);
            self.widest_line = (version, self.widest_line.1.max(visible));
            return self.widest_line.1;
        }
        if self.widest_line.0 != version {
            let buffer = self.state.buffer();
            let tab = self.state.settings.tab_size;
            let widest = (0..buffer.line_count())
                .map(|row| {
                    let line = buffer.line(row);
                    visual_col(&line, line.len(), tab)
                })
                .max()
                .unwrap_or(0);
            self.widest_line = (version, widest);
        }
        self.widest_line.1
    }
}
