//! Images for the image viewer: which files are images (by extension),
//! checking that their bytes decode, their pixel size, and SVGs drawn to a
//! PNG the viewer can show.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use resvg::{tiny_skia, usvg};

/// Image files larger than this are not opened (decoding needs several
/// times the file size in memory).
pub const MAX_IMAGE_SIZE: u64 = 256 * 1024 * 1024;

/// SVGs are drawn at twice their size (sharp on Retina screens), and at
/// most this many pixels on the long side.
const SVG_SCALE: f32 = 2.0;
const SVG_MAX_SIDE: f32 = 4096.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
    Bmp,
    Ico,
    Tiff,
    Svg,
}

impl ImageFormat {
    /// The format a file's extension names, if it is an image the viewer
    /// shows.
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "gif" => Self::Gif,
            "webp" => Self::Webp,
            "bmp" => Self::Bmp,
            "ico" => Self::Ico,
            "tif" | "tiff" => Self::Tiff,
            "svg" => Self::Svg,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
            Self::Webp => "WebP",
            Self::Bmp => "BMP",
            Self::Ico => "ICO",
            Self::Tiff => "TIFF",
            Self::Svg => "SVG",
        }
    }

    fn from_detected(format: image::ImageFormat) -> Option<Self> {
        Some(match format {
            image::ImageFormat::Png => Self::Png,
            image::ImageFormat::Jpeg => Self::Jpeg,
            image::ImageFormat::Gif => Self::Gif,
            image::ImageFormat::WebP => Self::Webp,
            image::ImageFormat::Bmp => Self::Bmp,
            image::ImageFormat::Ico => Self::Ico,
            image::ImageFormat::Tiff => Self::Tiff,
            _ => return None,
        })
    }
}

/// A decoded image, ready to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    /// The format of the file's contents (which may differ from its
    /// extension: a PNG saved as `.jpg` still opens).
    pub format: ImageFormat,
    /// Size in pixels (an SVG's own size, rounded up).
    pub width: u32,
    pub height: u32,
    /// What to draw: the file itself, or an SVG drawn to a PNG.
    pub bytes: Vec<u8>,
    /// The format of `bytes`.
    pub bytes_format: ImageFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("not a {0} image, or the file is damaged")]
    Unreadable(&'static str),
    #[error("{0} images are not supported")]
    Unsupported(String),
    #[error("the {format} data is damaged: {reason}")]
    Corrupt {
        format: &'static str,
        reason: String,
    },
    #[error("the image is empty (0 × 0 pixels)")]
    Empty,
}

/// Decodes `bytes` of a file whose extension names `hint`, checking the
/// whole image decodes. SVGs are drawn to a PNG.
pub fn decode(hint: ImageFormat, bytes: Vec<u8>) -> Result<DecodedImage, ImageError> {
    if hint == ImageFormat::Svg {
        return decode_svg(bytes);
    }
    let detected = image::guess_format(&bytes).map_err(|_| ImageError::Unreadable(hint.label()))?;
    let Some(format) = ImageFormat::from_detected(detected) else {
        let name = detected
            .extensions_str()
            .first()
            .map_or_else(|| format!("{detected:?}"), |e| e.to_ascii_uppercase());
        return Err(ImageError::Unsupported(name));
    };
    let image = image::load_from_memory_with_format(&bytes, detected).map_err(|error| {
        ImageError::Corrupt {
            format: format.label(),
            reason: error.to_string(),
        }
    })?;
    if image.width() == 0 || image.height() == 0 {
        return Err(ImageError::Empty);
    }
    Ok(DecodedImage {
        format,
        width: image.width(),
        height: image.height(),
        bytes,
        bytes_format: format,
    })
}

fn decode_svg(bytes: Vec<u8>) -> Result<DecodedImage, ImageError> {
    let corrupt = |reason: String| ImageError::Corrupt {
        format: "SVG",
        reason,
    };
    let mut options = usvg::Options::default();
    // System fonts take a while to load: only for SVGs with text.
    if bytes.windows(5).any(|w| w == b"<text") {
        options.fontdb = system_fonts();
    }
    let tree = usvg::Tree::from_data(&bytes, &options).map_err(|e| corrupt(e.to_string()))?;
    let size = tree.size();
    let (width, height) = (size.width().ceil() as u32, size.height().ceil() as u32);
    if width == 0 || height == 0 {
        return Err(ImageError::Empty);
    }
    let scale = SVG_SCALE.min(SVG_MAX_SIDE / size.width().max(size.height()));
    let mut pixmap = tiny_skia::Pixmap::new(
        ((size.width() * scale).ceil() as u32).max(1),
        ((size.height() * scale).ceil() as u32).max(1),
    )
    .ok_or(ImageError::Empty)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let png = pixmap.encode_png().map_err(|e| corrupt(e.to_string()))?;
    Ok(DecodedImage {
        format: ImageFormat::Svg,
        width,
        height,
        bytes: png,
        bytes_format: ImageFormat::Png,
    })
}

fn system_fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut fonts = usvg::fontdb::Database::new();
            fonts.load_system_fonts();
            Arc::new(fonts)
        })
        .clone()
}

/// `1.2 MB`-style size for the viewer's footer.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut size = bytes as f64 / 1024.0;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if size < 10.0 {
        format!("{size:.1} {}", UNITS[unit])
    } else {
        format!("{size:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A `width` × `height` RGBA image encoded as `format`.
    pub fn encoded(format: image::ImageFormat, width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
        let image = image::DynamicImage::ImageRgba8(image);
        // JPEG has no alpha channel.
        let image = if format == image::ImageFormat::Jpeg {
            image::DynamicImage::ImageRgb8(image.to_rgb8())
        } else {
            image
        };
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, format).expect("encode");
        out.into_inner()
    }

    #[test]
    fn formats_come_from_the_extension() {
        let format = |p: &str| ImageFormat::from_path(Path::new(p));
        assert_eq!(format("a/icon.PNG"), Some(ImageFormat::Png));
        assert_eq!(format("photo.jpeg"), Some(ImageFormat::Jpeg));
        assert_eq!(format("photo.jpg"), Some(ImageFormat::Jpeg));
        assert_eq!(format("logo.svg"), Some(ImageFormat::Svg));
        assert_eq!(format("scan.tif"), Some(ImageFormat::Tiff));
        assert_eq!(format("main.rs"), None);
        assert_eq!(format("Makefile"), None);
    }

    #[test]
    fn raster_images_decode_with_their_size() {
        for (format, expected) in [
            (image::ImageFormat::Png, ImageFormat::Png),
            (image::ImageFormat::Jpeg, ImageFormat::Jpeg),
            (image::ImageFormat::Gif, ImageFormat::Gif),
            (image::ImageFormat::Bmp, ImageFormat::Bmp),
            (image::ImageFormat::Ico, ImageFormat::Ico),
            (image::ImageFormat::Tiff, ImageFormat::Tiff),
        ] {
            let bytes = encoded(format, 3, 2);
            let decoded = decode(expected, bytes.clone()).expect("decodes");
            assert_eq!(decoded.format, expected);
            assert_eq!((decoded.width, decoded.height), (3, 2));
            assert_eq!(decoded.bytes, bytes);
        }
    }

    #[test]
    fn the_contents_win_over_the_extension() {
        let decoded = decode(ImageFormat::Jpeg, encoded(image::ImageFormat::Png, 4, 4))
            .expect("a PNG named .jpg");
        assert_eq!(decoded.format, ImageFormat::Png);
    }

    #[test]
    fn damaged_images_are_errors() {
        assert_eq!(
            decode(ImageFormat::Png, b"not an image at all".to_vec()),
            Err(ImageError::Unreadable("PNG"))
        );
        let mut truncated = encoded(image::ImageFormat::Png, 64, 64);
        truncated.truncate(truncated.len() / 2);
        let error = decode(ImageFormat::Png, truncated).expect_err("truncated");
        assert!(matches!(error, ImageError::Corrupt { format: "PNG", .. }));
        assert!(error.to_string().starts_with("the PNG data is damaged"));
    }

    #[test]
    fn svgs_are_drawn_to_png_at_twice_their_size() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="5"><rect width="10" height="5" fill="red"/></svg>"#;
        let decoded = decode(ImageFormat::Svg, svg.to_vec()).expect("svg");
        assert_eq!(decoded.format, ImageFormat::Svg);
        assert_eq!(decoded.bytes_format, ImageFormat::Png);
        assert_eq!((decoded.width, decoded.height), (10, 5));
        let png = image::load_from_memory(&decoded.bytes).expect("png");
        assert_eq!((png.width(), png.height()), (20, 10));
        assert!(matches!(
            decode(ImageFormat::Svg, b"<svg".to_vec()),
            Err(ImageError::Corrupt { format: "SVG", .. })
        ));
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(250 * 1024), "250 KB");
        assert_eq!(format_size(3 * 1024 * 1024), "3.0 MB");
    }
}
