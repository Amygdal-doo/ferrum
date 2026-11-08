//! Brand logos (`brand/png`), embedded so the toolbox needs no asset files.

use super::icon_for;
use ferrum_ui::gpui::{Image, ImageFormat, ObjectFit, StyledImage, img};
use ferrum_ui::prelude::*;
use ferrumtoolbox_core::AppId;
use std::sync::{Arc, OnceLock};

macro_rules! logo_bytes {
    ($size:literal) => {
        [
            include_bytes!(concat!("../../../../../brand/png/ide-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/api-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/git-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/data-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/term-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/toolbox-", $size, ".png")).as_slice(),
        ]
    };
}

const SMALL: [&[u8]; 6] = logo_bytes!("64");
const LARGE: [&[u8]; 6] = logo_bytes!("128");

fn index(app: AppId) -> usize {
    AppId::ALL.iter().position(|id| *id == app).unwrap_or(0)
}

fn images(cache: &'static OnceLock<Vec<Arc<Image>>>, bytes: &[&[u8]; 6]) -> &'static [Arc<Image>] {
    cache.get_or_init(|| {
        bytes
            .iter()
            .map(|bytes| Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec())))
            .collect()
    })
}

/// The app's logo at `size`, falling back to its icon if decoding fails.
pub fn logo(app: AppId, size: Pixels) -> impl IntoElement {
    static SMALL_IMAGES: OnceLock<Vec<Arc<Image>>> = OnceLock::new();
    static LARGE_IMAGES: OnceLock<Vec<Arc<Image>>> = OnceLock::new();
    let set = if size > px(48.) {
        images(&LARGE_IMAGES, &LARGE)
    } else {
        images(&SMALL_IMAGES, &SMALL)
    };
    let source = set.get(index(app)).cloned();
    div()
        .flex_none()
        .size(size)
        .flex()
        .items_center()
        .justify_center()
        .map(|this| match source {
            Some(image) => this.child(
                img(image)
                    .size(size)
                    .object_fit(ObjectFit::Contain)
                    .with_fallback(move || {
                        Icon::new(icon_for(app))
                            .color(Color::Muted)
                            .into_any_element()
                    }),
            ),
            None => this.child(Icon::new(icon_for(app)).color(Color::Muted)),
        })
}
