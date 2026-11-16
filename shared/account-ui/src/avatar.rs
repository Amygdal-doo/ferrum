//! The user's round avatar: the downloaded picture, else their initials.

use std::sync::Arc;

use ferrum_account::Me;
use ferrum_ui::gpui::{FontWeight, Image, ImageFormat, ObjectFit, StyledImage, img};
use ferrum_ui::prelude::*;
use ferrum_ui::theme::ActiveTheme;

/// A round avatar of `size`: `image` when given, else the initials.
pub fn avatar(user: &Me, image: Option<Arc<Image>>, size: Pixels, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let initials: SharedString = user.initials().into();
    let fallback = {
        let (bg, fg) = (colors.element_selected, colors.text);
        move || {
            div()
                .size_full()
                .rounded_full()
                .bg(bg)
                .text_color(fg)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .flex()
                .items_center()
                .justify_center()
                .child(initials.clone())
                .into_any_element()
        }
    };
    let circle = div()
        .flex_none()
        .size(size)
        .rounded_full()
        .overflow_hidden();
    match image {
        Some(image) => circle
            .child(
                img(image)
                    .size(size)
                    .rounded_full()
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(fallback),
            )
            .into_any_element(),
        None => circle.child(fallback()).into_any_element(),
    }
}

/// The image format from its first bytes (GitHub serves PNG or JPEG).
pub(crate) fn image_format(bytes: &[u8]) -> Option<ImageFormat> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some(ImageFormat::Png),
        [0xff, 0xd8, 0xff, ..] => Some(ImageFormat::Jpeg),
        [b'G', b'I', b'F', b'8', ..] => Some(ImageFormat::Gif),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some(ImageFormat::Webp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_avatar_formats() {
        assert_eq!(
            image_format(b"\x89PNG\r\n\x1a\n...."),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            image_format(&[0xff, 0xd8, 0xff, 0xe0, 0]),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(
            image_format(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(ImageFormat::Webp)
        );
        assert_eq!(image_format(b"<html>"), None);
        assert_eq!(image_format(b""), None);
    }
}
