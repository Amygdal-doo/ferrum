//! App logos: the brand logo compiled in for the built-in apps
//! (`brand/png`), else the catalog's `logo` PNG once downloaded (cached
//! under the data directory), else the app's icon.

use super::icon_for;
use crate::app::FerrumLauncher;
use ferrum_ui::gpui::{Image, ImageFormat, ObjectFit, StyledImage, img};
use ferrum_ui::prelude::*;
use ferrumlauncher_core::{AppId, store};
use std::sync::{Arc, OnceLock};

// In `AppId::BUILTIN` order.
macro_rules! logo_bytes {
    ($size:literal) => {
        [
            include_bytes!(concat!("../../../../../brand/png/editor-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/api-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/git-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/data-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/term-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/kit-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/dock-", $size, ".png")).as_slice(),
            include_bytes!(concat!("../../../../../brand/png/launcher-", $size, ".png")).as_slice(),
        ]
    };
}

const SMALL: [&[u8]; 8] = logo_bytes!("64");
const LARGE: [&[u8]; 8] = logo_bytes!("128");

fn images(cache: &'static OnceLock<Vec<Arc<Image>>>, bytes: &[&[u8]; 8]) -> &'static [Arc<Image>] {
    cache.get_or_init(|| {
        bytes
            .iter()
            .map(|bytes| Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec())))
            .collect()
    })
}

/// The compiled-in logo of a built-in app.
fn builtin_logo(app: AppId, size: Pixels) -> Option<Arc<Image>> {
    static SMALL_IMAGES: OnceLock<Vec<Arc<Image>>> = OnceLock::new();
    static LARGE_IMAGES: OnceLock<Vec<Arc<Image>>> = OnceLock::new();
    let set = if size > px(48.) {
        images(&LARGE_IMAGES, &LARGE)
    } else {
        images(&SMALL_IMAGES, &SMALL)
    };
    let index = AppId::BUILTIN.iter().position(|id| *id == app)?;
    set.get(index).cloned()
}

impl FerrumLauncher {
    /// The app's logo at `size`, falling back to its icon.
    pub fn logo(&self, app: AppId, size: Pixels) -> impl IntoElement + use<> {
        let icon = icon_for(&self.state.info(app).icon);
        // Built-in apps use the logo compiled into this launcher, so a cached
        // download never shows an old mark (it showed FeX after the rename).
        let source = builtin_logo(app, size).or_else(|| self.logos.get(&app).cloned());
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
                            Icon::new(icon).color(Color::Muted).into_any_element()
                        }),
                ),
                None => this.child(Icon::new(icon).color(Color::Muted)),
            })
    }

    /// Loads catalog logos: from the on-disk cache, downloading the ones
    /// not cached yet. Failures are logged; the built-in logo or the icon
    /// stays.
    pub fn load_logos(&mut self, cx: &mut Context<Self>) {
        let apps: Vec<_> = self
            .state
            .catalog()
            .apps()
            .into_iter()
            .filter(|info| {
                info.logo.is_some() && !info.id.is_builtin() && !self.logos.contains_key(&info.id)
            })
            .collect();
        if apps.is_empty() {
            return;
        }
        let services = self.services.clone();
        let settings = self.state.settings.clone();
        cx.spawn(async move |this, cx| {
            let paths = services.paths.clone();
            let loaded = services
                .run(async move {
                    // The logo URL isn't a GitHub API URL: no token needed.
                    let client = ferrumlauncher_core::GitHubClient::new(
                        &settings.repo_owner,
                        &settings.repo_name,
                        None,
                    )
                    .ok()?;
                    let mut loaded = Vec::new();
                    for info in apps {
                        match store::cache_logo(&client, &paths, &info).await {
                            Ok(Some(path)) => match tokio::fs::read(&path).await {
                                Ok(bytes) => loaded.push((info.id, bytes)),
                                Err(e) => tracing::warn!(app = %info.id, error = %e, "could not read cached logo"),
                            },
                            Ok(None) => {}
                            Err(e) => {
                                tracing::warn!(app = %info.id, error = %e, "could not download logo")
                            }
                        }
                    }
                    Some(loaded)
                })
                .await;
            let Ok(Some(loaded)) = loaded else { return };
            this.update(cx, |this, cx| {
                for (app, bytes) in loaded {
                    this.logos
                        .insert(app, Arc::new(Image::from_bytes(ImageFormat::Png, bytes)));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
