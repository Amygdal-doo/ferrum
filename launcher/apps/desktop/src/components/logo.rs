//! App logos: the brand logo compiled in for the built-in apps
//! (`brand/png`), else the catalog's `logo` PNG once downloaded (cached
//! under the data directory), else the app's icon.

use super::icon_for;
use crate::app::FerrumLauncher;
use ferrum_ui::gpui::{Image, ImageFormat, ObjectFit, StyledImage, img};
use ferrum_ui::prelude::*;
use ferrumlauncher_core::{AppId, store};
use std::sync::{Arc, OnceLock};

// Keyed by app, so reordering `AppId::BUILTIN` cannot mix the logos up.
macro_rules! logo_bytes {
    ($size:literal; $($app:ident => $name:literal),* $(,)?) => {
        [$((
            AppId::$app,
            include_bytes!(concat!("../../../../../brand/png/", $name, "-", $size, ".png")).as_slice(),
        )),*]
    };
}

macro_rules! logos {
    ($size:literal) => {
        logo_bytes!($size;
            EDITOR => "editor", VIBE => "vibe", GIT => "git", TERM => "term", API => "api",
            DOCK => "dock", DATA => "data", ENV => "env", PORTS => "ports", KIT => "kit",
            SYSTEM => "system", LAUNCHER => "launcher",
        )
    };
}

const LOGOS: usize = AppId::BUILTIN.len();
const SMALL: [(AppId, &[u8]); LOGOS] = logos!("64");
const LARGE: [(AppId, &[u8]); LOGOS] = logos!("128");

type Images = Vec<(AppId, Arc<Image>)>;

fn images(cache: &'static OnceLock<Images>, bytes: &[(AppId, &[u8]); LOGOS]) -> &'static Images {
    cache.get_or_init(|| {
        bytes
            .iter()
            .map(|(app, bytes)| {
                (
                    *app,
                    Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec())),
                )
            })
            .collect()
    })
}

/// The compiled-in logo of a built-in app.
fn builtin_logo(app: AppId, size: Pixels) -> Option<Arc<Image>> {
    static SMALL_IMAGES: OnceLock<Images> = OnceLock::new();
    static LARGE_IMAGES: OnceLock<Images> = OnceLock::new();
    let set = if size > px(48.) {
        images(&LARGE_IMAGES, &LARGE)
    } else {
        images(&SMALL_IMAGES, &SMALL)
    };
    set.iter()
        .find(|(id, _)| *id == app)
        .map(|(_, image)| image.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_app_gets_its_own_logo() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../brand/png");
        for (set, size) in [(&SMALL, 64), (&LARGE, 128)] {
            for app in AppId::BUILTIN {
                let (_, bytes) = set.iter().find(|(id, _)| *id == app).expect("logo");
                let file = std::fs::read(format!("{dir}/{app}-{size}.png")).expect("png");
                assert_eq!(*bytes, file.as_slice(), "{app} {size}");
            }
        }
    }
}
