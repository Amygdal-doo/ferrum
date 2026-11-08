//! FerrumTerm: a GPU terminal window (winit + wgpu + glyphon) on top of the
//! persistent FerrumTerm server. Tabs are the session's windows, splits are
//! its blocks. Close the window any time — agents keep running; reopen (or
//! `fterm attach` from anywhere, even over SSH) to pick up where you left off.

mod app;
mod gpu;
mod input;
mod view;

use app::{App, UiEvent};
use ferrumterm_protocol::Rect;
use glyphon::{TextArea, TextBounds};
use gpu::{Gpu, Quad};
use std::sync::Arc;
use std::time::Duration;
use view::{ACCENT, CellMetrics, DIVIDER};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

struct Handler {
    app: App,
    gpu: Option<Gpu>,
    texts: std::collections::HashMap<ferrumterm_protocol::BlockId, glyphon::Buffer>,
    modifiers: ModifiersState,
    cursor: (f32, f32),
    wheel: f32,
}

impl Handler {
    fn scale(&self) -> f32 {
        self.gpu
            .as_ref()
            .map_or(1.0, |g| g.window.scale_factor() as f32)
    }

    fn metrics(&mut self) -> Option<CellMetrics> {
        if self.app.metrics.is_none() {
            let scale = self.scale();
            let gpu = self.gpu.as_mut()?;
            self.app.metrics = Some(CellMetrics::measure(
                &mut gpu.font_system,
                (self.app.font_pt * scale).round(),
                scale,
            ));
            self.texts.clear();
            self.app.dirty.extend(self.app.frames.keys().copied());
        }
        self.app.metrics
    }

    fn sync(&mut self) {
        let cells = self
            .metrics()
            .zip(self.gpu.as_ref().map(Gpu::size))
            .map(|(m, (w, h))| m.grid(w, h));
        self.app.sync(cells);
        if let Some(g) = &self.gpu {
            let title = self.app.session().map_or_else(
                || "FerrumTerm".to_owned(),
                |s| {
                    let t = self
                        .app
                        .window()
                        .and_then(|w| w.blocks.iter().find(|b| b.id == w.focused))
                        .map(|b| b.title.clone())
                        .unwrap_or_default();
                    if t.is_empty() {
                        format!("FerrumTerm — {}", s.name)
                    } else {
                        format!("{t} — {}", s.name)
                    }
                },
            );
            g.window.set_title(&title);
            g.window.request_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(m) = self.metrics() else { return };
        let scale = self.scale();
        let Some(gpu) = self.gpu.as_mut() else { return };
        let (w, h) = gpu.size();
        for b in self.app.dirty.drain() {
            if let Some(f) = self.app.frames.get(&b) {
                self.texts
                    .insert(b, view::pane_text(&mut gpu.font_system, f, &m));
            }
        }
        let mut quads: Vec<Quad> = Vec::new();
        let (tab_texts, hits) = match self.app.session() {
            Some(s) => view::tab_bar(&mut gpu.font_system, s, &m, w, scale, &mut quads),
            None => (Vec::new(), Vec::new()),
        };
        self.app.tabs = hits;
        self.app.panes.clear();
        let mut areas: Vec<TextArea<'_>> = Vec::new();
        if let Some(win) = self.app.window().cloned() {
            let (ox, oy) = m.origin();
            let area = Rect {
                x: 0,
                y: 0,
                cols: win.cols,
                rows: win.rows,
            };
            for (b, r) in win.layout.rects(area) {
                let (px, py) = (
                    ox + f32::from(r.x) * m.cell_w,
                    oy + f32::from(r.y) * m.cell_h,
                );
                let (pw, ph) = (f32::from(r.cols) * m.cell_w, f32::from(r.rows) * m.cell_h);
                self.app.panes.push((b, px, py, pw, ph));
                let focused = b == win.focused;
                if let Some(f) = self.app.frames.get(&b) {
                    view::pane_quads(f, px, py, &m, focused, &mut quads);
                }
                // Divider in the gap cell to the right / below.
                let color = if focused && win.blocks.len() > 1 {
                    ACCENT
                } else {
                    DIVIDER
                };
                if r.x + r.cols < area.cols {
                    quads.push(Quad {
                        x: px + pw + m.cell_w / 2.0,
                        y: py,
                        w: 1.0 * scale,
                        h: ph,
                        rgb: color,
                        alpha: 1.0,
                    });
                }
                if r.y + r.rows < area.rows {
                    quads.push(Quad {
                        x: px,
                        y: py + ph + m.cell_h / 2.0,
                        w: pw,
                        h: 1.0 * scale,
                        rgb: color,
                        alpha: 1.0,
                    });
                }
            }
        }
        for (b, px, py, pw, ph) in &self.app.panes {
            if let Some(buf) = self.texts.get(b) {
                areas.push(TextArea {
                    buffer: buf,
                    left: *px,
                    top: *py,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: *px as i32,
                        top: *py as i32,
                        right: (px + pw) as i32,
                        bottom: (py + ph) as i32,
                    },
                    default_color: view::rgb(view::FG),
                    custom_glyphs: &[],
                });
            }
        }
        let text_top = (m.tab_h - (13.0 * scale * 1.3).round()) / 2.0;
        for (buf, x, color) in &tab_texts {
            areas.push(TextArea {
                buffer: buf,
                left: *x,
                top: text_top,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: w as i32,
                    bottom: m.tab_h as i32,
                },
                default_color: *color,
                custom_glyphs: &[],
            });
        }
        // Transient message strip at the bottom.
        let msg_buf = self
            .app
            .message
            .as_ref()
            .filter(|(_, t)| t.elapsed() < Duration::from_secs(6))
            .map(|(text, _)| {
                let mut b = glyphon::Buffer::new(
                    &mut gpu.font_system,
                    glyphon::Metrics::new(13.0 * scale, 18.0 * scale),
                );
                b.set_size(
                    &mut gpu.font_system,
                    Some(w - 40.0 * scale),
                    Some(20.0 * scale),
                );
                b.set_text(
                    &mut gpu.font_system,
                    text,
                    &glyphon::Attrs::new(),
                    glyphon::Shaping::Advanced,
                    None,
                );
                b.shape_until_scroll(&mut gpu.font_system, false);
                b
            });
        if let Some(b) = &msg_buf {
            quads.push(Quad {
                x: 0.0,
                y: h - 28.0 * scale,
                w,
                h: 28.0 * scale,
                rgb: 0x3a2a1a,
                alpha: 0.95,
            });
            areas.push(TextArea {
                buffer: b,
                left: 14.0 * scale,
                top: h - 23.0 * scale,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: (h - 28.0 * scale) as i32,
                    right: w as i32,
                    bottom: h as i32,
                },
                default_color: view::rgb(0xf0c674),
                custom_glyphs: &[],
            });
        }
        if let Err(e) = gpu.render(view::BG, &quads, areas) {
            tracing::warn!(error = %e, "render failed");
        }
    }
}

impl ApplicationHandler<UiEvent> for Handler {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("FerrumTerm")
            .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 700.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(420.0, 260.0));
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true)
                .with_title_hidden(true)
        };
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("FerrumTerm: could not open a window: {e}");
                el.exit();
                return;
            }
        };
        match pollster::block_on(Gpu::new(window)) {
            Ok(g) => self.gpu = Some(g),
            Err(e) => {
                eprintln!("FerrumTerm: GPU init failed: {e}");
                el.exit();
                return;
            }
        }
        self.app.connect(Duration::ZERO);
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, e: UiEvent) {
        match e {
            UiEvent::Connected(c) => {
                self.app.client = Some(c);
                self.app.subscribed.clear();
                self.app.refresh();
            }
            UiEvent::Server(ev) => {
                let frame = matches!(ev, ferrumterm_protocol::Event::Frame { .. });
                self.app.on_event(ev);
                if frame {
                    if let Some(g) = &self.gpu {
                        g.window.request_redraw();
                    }
                    return;
                }
            }
            UiEvent::Sessions(s) => self.app.set_sessions(s),
            UiEvent::Message(m) => self.app.notify(m),
            UiEvent::Disconnected(why) => {
                self.app.client = None;
                self.app.notify(format!("{why} — retrying…"));
                self.app.connect(Duration::from_secs(2));
            }
            UiEvent::Clipboard(text) => {
                match arboard::Clipboard::new().and_then(|mut c| c.set_text(text)) {
                    Ok(()) => self.app.notify("Copied screen to clipboard"),
                    Err(e) => self.app.notify(format!("Clipboard: {e}")),
                }
            }
        }
        self.sync();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                if let Some(g) = self.gpu.as_mut() {
                    g.resize(size.width, size.height);
                }
                self.sync();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.app.metrics = None;
                self.sync();
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(action) = input::translate(&event, self.modifiers) {
                    let font_change = self.app.font_pt;
                    self.app.act(action);
                    if (self.app.font_pt - font_change).abs() > f32::EPSILON
                        || self.app.metrics.is_none()
                    {
                        self.sync();
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32)
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.app.click(self.cursor.0, self.cursor.1);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let cell_h = self.app.metrics.map_or(16.0, |m| m.cell_h);
                self.wheel += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 3.0,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / cell_h,
                };
                let lines = self.wheel.trunc();
                self.wheel -= lines;
                self.app.scroll(lines as i32);
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("FerrumTerm: {e}");
            std::process::exit(1);
        }
    };
    let event_loop = match EventLoop::<UiEvent>::with_user_event().build() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("FerrumTerm: {e}");
            std::process::exit(1);
        }
    };
    let app = App::new(rt, event_loop.create_proxy());
    let mut handler = Handler {
        app,
        gpu: None,
        texts: Default::default(),
        modifiers: ModifiersState::empty(),
        cursor: (0.0, 0.0),
        wheel: 0.0,
    };
    if let Err(e) = event_loop.run_app(&mut handler) {
        eprintln!("FerrumTerm: {e}");
    }
}
