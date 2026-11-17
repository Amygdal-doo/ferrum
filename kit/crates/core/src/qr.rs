//! QR codes: encode text (the `qrcode` crate) and render it as a PNG
//! (the `image` crate), black on white with the standard quiet zone.

use std::io::Cursor;

use image::{ImageFormat, Luma, imageops};
use qrcode::{Color, EcLevel, QrCode};

/// Error correction: how much of the code may be damaged and still read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Correction {
    Low,
    Medium,
    Quartile,
    High,
}

impl Correction {
    pub const ALL: [Correction; 4] = [
        Correction::Low,
        Correction::Medium,
        Correction::Quartile,
        Correction::High,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Correction::Low => "L 7%",
            Correction::Medium => "M 15%",
            Correction::Quartile => "Q 25%",
            Correction::High => "H 30%",
        }
    }

    fn level(self) -> EcLevel {
        match self {
            Correction::Low => EcLevel::L,
            Correction::Medium => EcLevel::M,
            Correction::Quartile => EcLevel::Q,
            Correction::High => EcLevel::H,
        }
    }
}

/// Modules (squares) of empty border on each side, as the spec asks.
pub const QUIET_ZONE: usize = 4;

/// An encoded code: `width × width` modules, `true` for dark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Qr {
    pub width: usize,
    pub modules: Vec<bool>,
}

/// Encodes the UTF-8 bytes of `text`.
pub fn encode(text: &str, correction: Correction) -> Result<Qr, String> {
    if text.is_empty() {
        return Err("Enter text to encode".into());
    }
    let code =
        QrCode::with_error_correction_level(text.as_bytes(), correction.level()).map_err(|e| {
            match e {
                qrcode::types::QrError::DataTooLong => format!(
                    "Too long for a QR code at {} error correction ({} bytes)",
                    correction.label(),
                    text.len()
                ),
                other => other.to_string(),
            }
        })?;
    Ok(Qr {
        width: code.width(),
        modules: code
            .to_colors()
            .into_iter()
            .map(|c| c == Color::Dark)
            .collect(),
    })
}

impl Qr {
    /// A PNG with each module `scale` pixels wide, quiet zone included.
    pub fn to_png(&self, scale: u32) -> Result<Vec<u8>, String> {
        let side = (self.width + 2 * QUIET_ZONE) as u32;
        let small = image::GrayImage::from_fn(side, side, |x, y| {
            let (x, y) = (x as usize, y as usize);
            let inside = (QUIET_ZONE..QUIET_ZONE + self.width).contains(&x)
                && (QUIET_ZONE..QUIET_ZONE + self.width).contains(&y);
            let dark = inside && self.modules[(y - QUIET_ZONE) * self.width + x - QUIET_ZONE];
            Luma([if dark { 0 } else { 255 }])
        });
        let scale = scale.max(1);
        let image = imageops::resize(
            &small,
            side * scale,
            side * scale,
            imageops::FilterType::Nearest,
        );
        let mut png = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok(png)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_text() {
        let qr = encode("https://ferrum.dev", Correction::Medium).unwrap();
        // Version 2 (25 × 25) holds this at level M.
        assert_eq!(qr.width, 25);
        assert_eq!(qr.modules.len(), 25 * 25);
        // The finder pattern's corner is dark.
        assert!(qr.modules[0]);
        let high = encode("https://ferrum.dev", Correction::High).unwrap();
        assert!(high.width >= qr.width);
    }

    #[test]
    fn errors() {
        assert!(encode("", Correction::Low).is_err());
        let long = "x".repeat(3000);
        assert!(
            encode(&long, Correction::High)
                .unwrap_err()
                .contains("Too long")
        );
    }

    #[test]
    fn renders_a_png_with_quiet_zone() {
        let qr = encode("hi", Correction::Low).unwrap();
        let png = qr.to_png(4).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let image = image::load_from_memory(&png).unwrap().to_luma8();
        let side = (qr.width + 2 * QUIET_ZONE) as u32 * 4;
        assert_eq!(image.dimensions(), (side, side));
        // Quiet zone white, first module dark.
        assert_eq!(image.get_pixel(0, 0).0, [255]);
        let first = (QUIET_ZONE * 4) as u32;
        assert_eq!(image.get_pixel(first, first).0, [0]);
    }
}
