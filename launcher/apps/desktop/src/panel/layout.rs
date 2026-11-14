//! The panel's pure logic: where it opens under the menu bar icon and what
//! each app row shows.

use ferrum_ui::gpui::{Bounds, Pixels, Point, Size, point, px, size};
use semver::Version;

/// The panel's size.
pub const PANEL_SIZE: Size<Pixels> = Size {
    width: px(380.),
    height: px(560.),
};
/// Space between the menu bar and the panel.
const GAP: f32 = 4.;
/// Space kept between the panel and the screen's edges.
const MARGIN: f32 = 8.;

/// The menu bar icon's frame as the tray reports it: physical pixels, from
/// the top-left of the main display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl IconRect {
    /// The display scale behind the icon. The menu bar is 22–24pt tall
    /// (37pt beside a notch, only on Retina screens), so its physical
    /// height tells 1× from 2× (and 3×).
    #[must_use]
    pub fn scale(&self) -> f64 {
        match self.height {
            h if h < 36. => 1.,
            h if h < 90. => 2.,
            _ => 3.,
        }
    }

    /// The frame in points.
    #[must_use]
    pub fn logical(&self) -> Bounds<Pixels> {
        let s = self.scale();
        Bounds {
            origin: point(px((self.x / s) as f32), px((self.y / s) as f32)),
            size: size(px((self.width / s) as f32), px((self.height / s) as f32)),
        }
    }
}

/// Top-left of a panel of `panel` size under `icon` (both in points), on
/// `screen` (the main display's frame). Centred on the icon and kept on
/// the screen; an icon on another display (whose frame isn't known) gets
/// the panel right-aligned to it, so it opens towards the screen's middle.
#[must_use]
pub fn panel_origin(
    icon: Bounds<Pixels>,
    screen: Bounds<Pixels>,
    panel: Size<Pixels>,
) -> Point<Pixels> {
    let center = icon.origin.x + icon.size.width / 2.;
    let y = icon.origin.y + icon.size.height + px(GAP);
    let on_screen = center >= screen.origin.x
        && center < screen.origin.x + screen.size.width
        && icon.origin.y >= screen.origin.y
        && icon.origin.y < screen.origin.y + screen.size.height;
    if !on_screen {
        return point(icon.origin.x + icon.size.width - panel.width, y);
    }
    let min = screen.origin.x + px(MARGIN);
    let max = screen.origin.x + screen.size.width - px(MARGIN) - panel.width;
    let x = (center - panel.width / 2.).min(max).max(min);
    point(x, y)
}

/// What an app row shows besides the name.
#[derive(Debug, Clone, PartialEq)]
pub enum RowState {
    /// Installed and current: the version.
    UpToDate(Version),
    /// Installed with a newer version out: `from → to` and Update.
    Update { from: Version, to: Version },
    /// A download running: progress and Cancel.
    Downloading {
        from: Option<Version>,
        to: Version,
        fraction: Option<f32>,
    },
    /// Not installed, with a build for this computer: Install.
    Install(Version),
    /// Not installed and nothing to install here.
    Unavailable,
}

impl RowState {
    /// From the app's active version, its pending update, the newest
    /// release with a build for this platform and a running download
    /// (version and fraction).
    #[must_use]
    pub fn new(
        active: Option<&Version>,
        update: Option<&Version>,
        installable: Option<&Version>,
        download: Option<(&Version, Option<f32>)>,
    ) -> Self {
        if let Some((to, fraction)) = download {
            return Self::Downloading {
                from: active.cloned(),
                to: to.clone(),
                fraction,
            };
        }
        match (active, update, installable) {
            (Some(from), Some(to), _) => Self::Update {
                from: from.clone(),
                to: to.clone(),
            },
            (Some(version), None, _) => Self::UpToDate(version.clone()),
            (None, _, Some(version)) => Self::Install(version.clone()),
            (None, _, None) => Self::Unavailable,
        }
    }

    /// Listed under Installed (rather than Available).
    #[must_use]
    pub fn installed(&self) -> bool {
        match self {
            Self::UpToDate(_) | Self::Update { .. } => true,
            Self::Downloading { from, .. } => from.is_some(),
            Self::Install(_) | Self::Unavailable => false,
        }
    }
}

/// Whether `text` matches the search `query` (case-insensitive; an empty
/// query matches everything).
#[must_use]
pub fn matches(query: &str, text: &str) -> bool {
    let query = query.trim();
    query.is_empty() || text.to_lowercase().contains(&query.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).expect("version")
    }

    fn screen() -> Bounds<Pixels> {
        Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(1512.), px(982.)),
        }
    }

    fn icon(x: f32, width: f32) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(x), px(0.)),
            size: size(px(width), px(24.)),
        }
    }

    #[test]
    fn physical_rects_become_points() {
        let retina = IconRect {
            x: 2400.,
            y: 0.,
            width: 60.,
            height: 74.,
        };
        assert_eq!(retina.scale(), 2.);
        assert_eq!(
            retina.logical(),
            Bounds {
                origin: point(px(1200.), px(0.)),
                size: size(px(30.), px(37.)),
            }
        );
        let plain = IconRect {
            x: 900.,
            y: 0.,
            width: 30.,
            height: 24.,
        };
        assert_eq!(plain.scale(), 1.);
        assert_eq!(plain.logical().origin.x, px(900.));
        assert_eq!(
            IconRect {
                height: 48.,
                ..plain
            }
            .scale(),
            2.
        );
    }

    #[test]
    fn panel_centres_under_the_icon() {
        let origin = panel_origin(icon(700., 30.), screen(), PANEL_SIZE);
        assert_eq!(origin, point(px(715. - 190.), px(28.)));
    }

    #[test]
    fn panel_stays_on_screen() {
        // Near the right edge: pulled left to keep the margin.
        let right = panel_origin(icon(1480., 30.), screen(), PANEL_SIZE);
        assert_eq!(right.x, px(1512. - 8. - 380.));
        // Near the left edge: pushed right.
        let left = panel_origin(icon(10., 30.), screen(), PANEL_SIZE);
        assert_eq!(left.x, px(8.));
    }

    #[test]
    fn panel_on_another_display_aligns_to_the_icon() {
        let origin = panel_origin(icon(2900., 30.), screen(), PANEL_SIZE);
        assert_eq!(origin, point(px(2930. - 380.), px(28.)));
    }

    #[test]
    fn row_states() {
        let (old, new) = (v("0.7.0-alpha.1"), v("0.7.1-alpha.1"));
        assert_eq!(
            RowState::new(Some(&old), None, Some(&new), None),
            RowState::UpToDate(old.clone())
        );
        assert_eq!(
            RowState::new(Some(&old), Some(&new), Some(&new), None),
            RowState::Update {
                from: old.clone(),
                to: new.clone()
            }
        );
        let downloading = RowState::new(Some(&old), Some(&new), None, Some((&new, Some(0.5))));
        assert_eq!(
            downloading,
            RowState::Downloading {
                from: Some(old.clone()),
                to: new.clone(),
                fraction: Some(0.5)
            }
        );
        assert!(downloading.installed());
        let first = RowState::new(None, None, Some(&new), Some((&new, None)));
        assert!(!first.installed());
        assert_eq!(
            RowState::new(None, None, Some(&new), None),
            RowState::Install(new.clone())
        );
        assert_eq!(RowState::new(None, None, None, None), RowState::Unavailable);
        assert!(!RowState::Unavailable.installed());
    }

    #[test]
    fn search_matches_case_insensitively() {
        assert!(matches("", "FerrumGIT"));
        assert!(matches(" git ", "FerrumGIT"));
        assert!(!matches("term", "FerrumGIT"));
    }
}
