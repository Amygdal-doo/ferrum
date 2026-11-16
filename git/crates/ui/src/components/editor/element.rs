//! The editor surface element. Prepaint shapes only the visible rows and
//! builds every quad (see `decorations`); paint draws them and registers
//! the IME input handler.

use ferrum_ui::theme::{ActiveTheme as _, theme_settings};
use ferrumeditor_core::display::{ExpandedLine, visual_col};
use gpui::{
    App, Bounds, ContentMask, ElementId, ElementInputHandler, Entity, Font, GlobalElementId, Hsla,
    InspectorElementId, LayoutId, PaintQuad, Pixels, Point, ShapedLine, SharedString, Style,
    TextAlign, TextRun, Window, fill, point, prelude::*, px, relative, size,
};

use super::CodeEditor;
use super::layout::{LayoutSnapshot, RowLayout, RunInput, build_runs, hsla};

const SCROLLBAR_WIDTH: Pixels = px(10.);
const LINE_HEIGHT: f32 = 1.5;

pub struct EditorElement {
    editor: Entity<CodeEditor>,
}

impl EditorElement {
    pub fn new(editor: Entity<CodeEditor>) -> Self {
        Self { editor }
    }
}

/// Everything prepaint computed for paint.
pub struct Frame {
    pub(super) layout: LayoutSnapshot,
    gutter: Bounds<Pixels>,
    pub(super) text_area: Bounds<Pixels>,
    editor_bg: Hsla,
    gutter_bg: Hsla,
    pub(super) backgrounds: Vec<PaintQuad>,
    pub(super) foreground: Vec<PaintQuad>,
    pub(super) cursors: Vec<PaintQuad>,
    pub(super) numbers: Vec<(ShapedLine, Point<Pixels>)>,
    scrollbar: Option<PaintQuad>,
}

/// Font metrics resolved once per frame.
pub(super) struct FrameStyle {
    pub(super) font: Font,
    pub(super) font_size: Pixels,
    line_height: Pixels,
    char_width: Pixels,
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
        let settings = theme_settings(cx);
        let font = settings.buffer_font.clone();
        let font_size = settings.buffer_font_size;
        let text_system = window.text_system();
        let char_width = text_system
            .advance(text_system.resolve_font(&font), font_size, 'm')
            .map_or(font_size * 0.6, |advance| advance.width);
        let style = FrameStyle {
            font,
            font_size,
            line_height: (font_size * LINE_HEIGHT).round(),
            char_width,
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
        window.paint_quad(fill(frame.gutter, frame.gutter_bg));
        window.paint_quad(fill(frame.text_area, frame.editor_bg));
        let line_height = frame.layout.line_height;
        let text_mask = ContentMask {
            bounds: frame.text_area,
        };
        window.with_content_mask(Some(text_mask), |window| {
            for quad in frame.backgrounds.drain(..) {
                window.paint_quad(quad);
            }
            let origin_x = frame.layout.text_left - frame.layout.scroll_x;
            for row in &frame.layout.rows {
                let origin = point(origin_x, row.top);
                if let Err(error) =
                    row.shaped
                        .paint(origin, line_height, TextAlign::Left, None, window, cx)
                {
                    tracing::warn!(%error, "failed to paint an editor line");
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
        });
        let gutter_mask = ContentMask {
            bounds: frame.gutter,
        };
        window.with_content_mask(Some(gutter_mask), |window| {
            for (number, origin) in &frame.numbers {
                if let Err(error) =
                    number.paint(*origin, line_height, TextAlign::Left, None, window, cx)
                {
                    tracing::warn!(%error, "failed to paint a line number");
                }
            }
        });
        if let Some(scrollbar) = frame.scrollbar.take() {
            window.paint_quad(scrollbar);
        }
    }
}

impl CodeEditor {
    fn prepaint_frame(
        &mut self,
        bounds: Bounds<Pixels>,
        style: &FrameStyle,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Frame {
        let tab_size = self.state.settings.tab_size;
        let (line_height, char_width) = (style.line_height, style.char_width);
        let line_count = self.state.buffer().line_count();
        let digits = line_count.to_string().len().max(2) as f32;
        let gutter_width = char_width * (digits + 1.) + px(16.);
        let text_left = bounds.left() + gutter_width + px(6.);
        let text_width =
            (bounds.size.width - gutter_width - px(6.) - SCROLLBAR_WIDTH).max(char_width * 10.);
        self.display_map.sync(self.state.buffer(), tab_size, None);

        // Vertical scroll: a requested line a third of the way down, else
        // the primary cursor kept in view after commands.
        let viewport = bounds.size.height;
        let head = self.state.primary().head;
        if let Some(row) = self.pending_line.take() {
            self.scroll_y = (line_height * row as f32 - viewport / 3.).max(px(0.));
        } else if self.autoscroll {
            let top = line_height * self.state.buffer().row_of(head) as f32;
            let margin = (line_height * 3.).min(viewport / 4.);
            if top < self.scroll_y + margin {
                self.scroll_y = top - margin;
            } else if top + line_height > self.scroll_y + viewport - margin {
                self.scroll_y = top + line_height - viewport + margin;
            }
        }
        let max_scroll = line_height * line_count.saturating_sub(1) as f32;
        self.scroll_y = self.scroll_y.clamp(px(0.), max_scroll);
        let first_row = (self.scroll_y / line_height).floor() as usize;
        let visible = (viewport / line_height).ceil() as usize + 1;
        let rows = self.shape_rows(
            first_row..(first_row + visible).min(line_count),
            bounds,
            style,
            window,
            cx,
        );

        // Horizontal scroll.
        let widest = self.widest_line_cells();
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
        self.autoscroll = false;

        let colors = cx.theme().colors();
        let mut frame = Frame {
            layout: LayoutSnapshot {
                bounds,
                text_left,
                gutter_width,
                line_height,
                char_width,
                scroll_y: self.scroll_y,
                scroll_x: self.scroll_x,
                rows,
            },
            gutter: Bounds::new(bounds.origin, size(gutter_width, bounds.size.height)),
            text_area: Bounds::new(
                point(bounds.left() + gutter_width, bounds.top()),
                size(bounds.size.width - gutter_width, bounds.size.height),
            ),
            editor_bg: colors.editor_background,
            gutter_bg: colors.editor_gutter_background,
            backgrounds: Vec::new(),
            foreground: Vec::new(),
            cursors: Vec::new(),
            numbers: Vec::new(),
            scrollbar: None,
        };
        let current_line = colors.editor_active_line_background;
        let numbers = (colors.editor_line_number, colors.editor_active_line_number);
        self.decorate(&mut frame, current_line);
        self.line_numbers(&mut frame, style, numbers, window);
        if line_count > 1 && max_scroll > px(0.) {
            let content = line_height * line_count as f32 + viewport - line_height;
            let thumb = (viewport * (viewport / content)).max(px(24.));
            let y = bounds.top() + (viewport - thumb) * (self.scroll_y / max_scroll);
            let thumb_bounds = Bounds::new(
                point(bounds.right() - SCROLLBAR_WIDTH + px(2.), y),
                size(px(6.), thumb),
            );
            let color = hsla(self.theme.ui("scrollbar"));
            frame.scrollbar = Some(fill(thumb_bounds, color).corner_radii(px(3.)));
        }
        self.last_layout = Some(frame.layout.clone());
        frame
    }

    fn shape_rows(
        &self,
        rows: std::ops::Range<usize>,
        bounds: Bounds<Pixels>,
        style: &FrameStyle,
        window: &mut Window,
        cx: &App,
    ) -> Vec<RowLayout> {
        let buffer = self.state.buffer();
        let tab_size = self.state.settings.tab_size;
        let text_color = cx.theme().colors().editor_foreground;
        rows.map(|row| {
            let expanded = ExpandedLine::new(&buffer.line(row), tab_size);
            let line_start = buffer.line_start(row);
            let line_len = buffer.line_len(row);
            let marked = self
                .marked_range
                .clone()
                .filter(|m| m.start >= line_start && m.end <= line_start + line_len);
            let runs: Vec<TextRun> = build_runs(
                &RunInput {
                    theme: &self.theme,
                    font: &style.font,
                    text_color,
                    highlights: &self.highlights,
                    line_start,
                    line_len,
                    expanded: &expanded,
                    marked,
                },
                expanded.text.len(),
            );
            let text: SharedString = expanded.text.clone().into();
            let shaped = window
                .text_system()
                .shape_line(text, style.font_size, &runs, None);
            RowLayout {
                row,
                line_start,
                line_len,
                expanded,
                shaped,
                top: bounds.top() + style.line_height * row as f32 - self.scroll_y,
            }
        })
        .collect()
    }

    /// Widest line in cells, for the horizontal scroll range. Buffers over
    /// 1 MB only grow the estimate from the rows laid out.
    fn widest_line_cells(&mut self) -> usize {
        let buffer = self.state.buffer();
        let version = buffer.version();
        if self.widest_line.0 != version && buffer.len() <= 1024 * 1024 {
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
