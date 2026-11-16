//! Viewer pane item for files the text editor can't show: images (PNG,
//! JPEG, GIF, WebP, BMP, ICO, TIFF, SVG) fit to the pane with zoom, and
//! other binary files, which offer "Open with Default App" and "Reveal in
//! Finder" instead of a dead end.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use ferrum_ui::prelude::*;
use ferrumeditor_core::images::{self, DecodedImage, ImageFormat};
use gpui::Size;

use crate::actions::RunCommand;

mod render;

/// Zoom steps for ⌘+ / ⌘-.
const ZOOM_STEPS: [f32; 13] = [
    0.1, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0,
];
/// Space around the image (each side) when fitting it to the pane.
const FIT_PADDING: f32 = 16.0;

pub enum FileViewerEvent {
    /// "Open as Text" on an SVG.
    OpenAsText(PathBuf),
}

#[derive(Clone)]
pub enum ViewerContent {
    /// An image being read and decoded in the background.
    Loading,
    Image {
        info: ImageInfo,
        image: Arc<gpui::Image>,
    },
    /// An image that could not be shown, and why.
    Failed(SharedString),
    /// Not an image: offer other apps.
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
}

pub struct FileViewer {
    pub path: PathBuf,
    /// The format the extension names; `None` for other binary files.
    pub format: Option<ImageFormat>,
    pub content: ViewerContent,
    pub file_size: Option<u64>,
    /// `None` fits the image to the pane (never enlarging it).
    pub zoom: Option<f32>,
    /// The pane's size at the last paint, for zooming from the fitted size.
    viewport: Rc<Cell<Size<Pixels>>>,
    focus_handle: FocusHandle,
    _load: Option<Task<()>>,
}

impl EventEmitter<FileViewerEvent> for FileViewer {}

impl Focusable for FileViewer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FileViewer {
    pub fn new(path: PathBuf, cx: &mut Context<Self>) -> Self {
        let format = ImageFormat::from_path(&path);
        let mut viewer = Self {
            path,
            format,
            content: ViewerContent::Binary,
            file_size: None,
            zoom: None,
            viewport: Rc::default(),
            focus_handle: cx.focus_handle(),
            _load: None,
        };
        viewer.reload(cx);
        viewer
    }

    pub fn title(&self) -> SharedString {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.display().to_string())
            .into()
    }

    pub fn is_image(&self) -> bool {
        self.format.is_some()
    }

    /// Re-reads the file (after it changed on disk); the zoom is kept.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.file_size = std::fs::metadata(&self.path).ok().map(|m| m.len());
        let Some(format) = self.format else {
            self.content = ViewerContent::Binary;
            cx.notify();
            return;
        };
        if self
            .file_size
            .is_some_and(|size| size > images::MAX_IMAGE_SIZE)
        {
            self.content = ViewerContent::Failed(
                format!(
                    "The image is larger than {}.",
                    images::format_size(images::MAX_IMAGE_SIZE)
                )
                .into(),
            );
            cx.notify();
            return;
        }
        if matches!(self.content, ViewerContent::Binary) {
            self.content = ViewerContent::Loading;
        }
        let path = self.path.clone();
        let decoded = cx.background_spawn(async move {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            images::decode(format, bytes).map_err(|e| e.to_string())
        });
        self._load = Some(cx.spawn(async move |this, cx| {
            let decoded = decoded.await;
            this.update(cx, |this, cx| {
                this.content = match decoded {
                    Ok(decoded) => image_content(decoded),
                    Err(error) => ViewerContent::Failed(capitalized(&error).into()),
                };
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        let current = self.scale();
        let next = ZOOM_STEPS.iter().copied().find(|z| *z > current + 0.001);
        self.zoom = Some(next.unwrap_or(ZOOM_STEPS[ZOOM_STEPS.len() - 1]));
        cx.notify();
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        let current = self.scale();
        let next = ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|z| *z < current - 0.001);
        self.zoom = Some(next.unwrap_or(ZOOM_STEPS[0]));
        cx.notify();
    }

    pub fn zoom_to_fit(&mut self, cx: &mut Context<Self>) {
        self.zoom = None;
        cx.notify();
    }

    /// The scale the image is shown at: the zoom, or the fitted scale.
    pub fn scale(&self) -> f32 {
        if let Some(zoom) = self.zoom {
            return zoom;
        }
        let ViewerContent::Image { info, .. } = &self.content else {
            return 1.0;
        };
        let viewport = self.viewport.get();
        let room = |side: Pixels| (f32::from(side) - 2.0 * FIT_PADDING).max(1.0);
        if viewport.width <= px(0.) || viewport.height <= px(0.) {
            return 1.0;
        }
        (room(viewport.width) / info.width as f32)
            .min(room(viewport.height) / info.height as f32)
            .min(1.0)
    }

    /// Zoom commands reuse the font-size keys (⌘+ / ⌘- / ⌘0) while the
    /// viewer has focus; everything else bubbles up.
    fn on_run_command(&mut self, action: &RunCommand, _: &mut Window, cx: &mut Context<Self>) {
        match action.name.as_ref() {
            "window:increase-font-size" if self.is_image() => self.zoom_in(cx),
            "window:decrease-font-size" if self.is_image() => self.zoom_out(cx),
            "window:reset-font-size" if self.is_image() => self.zoom_to_fit(cx),
            _ => cx.propagate(),
        }
    }
}

fn image_content(decoded: DecodedImage) -> ViewerContent {
    let format = match decoded.bytes_format {
        ImageFormat::Png => gpui::ImageFormat::Png,
        ImageFormat::Jpeg => gpui::ImageFormat::Jpeg,
        ImageFormat::Gif => gpui::ImageFormat::Gif,
        ImageFormat::Webp => gpui::ImageFormat::Webp,
        ImageFormat::Bmp => gpui::ImageFormat::Bmp,
        ImageFormat::Ico => gpui::ImageFormat::Ico,
        ImageFormat::Tiff => gpui::ImageFormat::Tiff,
        ImageFormat::Svg => gpui::ImageFormat::Svg,
    };
    ViewerContent::Image {
        info: ImageInfo {
            format: decoded.format,
            width: decoded.width,
            height: decoded.height,
        },
        image: Arc::new(gpui::Image::from_bytes(format, decoded.bytes)),
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
