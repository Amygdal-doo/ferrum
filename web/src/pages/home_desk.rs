//! The home hero's desktop: one app window over a dock of every app.
//!
//! Every demo is rendered here, one per app, and all but FerrumIDE carry
//! `hidden`, so the window never changes size. With JS (`site.js`,
//! `[data-desk]`), the dock buttons switch the window, and it steps through
//! the Apps group until someone uses the dock. Without JS the buttons are
//! hidden and each dock icon is a plain link to its app's page.

use maud::{Markup, html};

use super::feature_demos;
use crate::products::{self, Group, Product};
use crate::routes::assets;

/// The app the window shows first, and the only one shown without JS.
const FIRST: Product = products::FERRUMIDE;

/// Id of the window's panel for an app, also usable as a URL hash (`#app-ferrumgit`).
fn panel_id(p: &Product) -> String {
    format!("app-{}", p.slug)
}

pub fn render() -> Markup {
    let groups: Vec<Vec<&Product>> = Group::ALL
        .iter()
        .map(|g| products::in_group(*g).collect())
        .collect();
    html! {
        div class="desk" data-desk {
            div class="desk__window" {
                div class="desk__bar" {
                    span class="demo__dots" aria-hidden="true" { i {} i {} i {} }
                    span class="desk__title" data-desk-title { (FIRST.name) }
                }
                div class="desk__screen" {
                    @for p in products::apps() {
                        div class="desk__app" id=(panel_id(p)) style=(p.accent_style())
                            role="img" aria-label={ "Animated preview of " (p.name) }
                            data-desk-app=(p.slug) data-name=(p.name)
                            hidden[p.slug != FIRST.slug] {
                            @if let Some(demo) = feature_demos::for_slug(p.slug) { (demo) }
                        }
                    }
                }
            }
            @for p in products::apps() {
                p class="desk__caption" data-desk-caption=(p.slug) hidden[p.slug != FIRST.slug] {
                    (p.tagline) " "
                    a href=(p.path()) { "More about " (p.name) }
                }
            }
            ul class="appdock" aria-label="Ferrum apps" {
                @for (i, group) in groups.iter().enumerate() {
                    @if i > 0 { li class="appdock__sep" aria-hidden="true" {} }
                    @for p in group { (dock_item(p)) }
                }
            }
        }
    }
}

/// One dock icon: a button that switches the window (JS) and a link to the
/// app's page (no JS). CSS shows one of the two from the `js` class on <html>.
fn dock_item(p: &Product) -> Markup {
    let logo = assets::url(&p.logo_asset());
    let first = p.slug == FIRST.slug;
    html! {
        li class="appdock__item" style=(p.accent_style()) {
            button class="appdock__btn" type="button" data-desk-show=(p.slug)
                aria-controls=(panel_id(p)) aria-pressed=(if first { "true" } else { "false" }) {
                img src=(logo) alt="" width="56" height="56";
                span class="appdock__name" { (p.name) }
            }
            a class="appdock__link" href=(p.path()) {
                img src=(logo) alt="" width="56" height="56";
                span class="appdock__name" { (p.name) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> String {
        render().into_string()
    }

    #[test]
    fn one_window_with_every_app_and_only_the_ide_shown() {
        let html = rendered();
        let apps = products::apps().count();
        assert_eq!(apps, 11);
        assert_eq!(html.matches(r#"class="desk__window""#).count(), 1);
        assert_eq!(html.matches(r#"class="desk__app""#).count(), apps);
        assert_eq!(html.matches(r#"class="demo""#).count(), apps);
        // Every panel and caption but FerrumIDE's is hidden.
        assert_eq!(html.matches(" hidden>").count(), (apps - 1) * 2);
        assert!(html.contains(r#"id="app-ferrumide""#));
        assert!(html.contains(r#"data-desk-title>FerrumIDE<"#));
    }

    #[test]
    fn dock_has_a_button_and_a_link_per_app_in_group_order() {
        let html = rendered();
        assert_eq!(html.matches(r#"class="appdock__btn""#).count(), 11);
        assert_eq!(html.matches(r#"class="appdock__link""#).count(), 11);
        assert_eq!(html.matches(r#"aria-pressed="true""#).count(), 1);
        assert_eq!(html.matches(r#"class="appdock__sep""#).count(), 1);
        for p in products::apps() {
            assert!(
                html.contains(&format!(r#"aria-controls="app-{}""#, p.slug)),
                "{}",
                p.slug
            );
            assert!(
                html.contains(&format!(r#"class="appdock__link" href="{}""#, p.path())),
                "{}",
                p.slug
            );
            assert!(html.contains(p.tagline), "{}", p.slug);
            assert!(
                html.contains(&format!("More about {}</a>", p.name)),
                "{}",
                p.slug
            );
        }
        // Apps first, then the divider, then the tools.
        let sep = html.find("appdock__sep").expect("divider");
        let pos = |p: Product| {
            html.find(&format!(r#"data-desk-show="{}""#, p.slug))
                .expect("dock button")
        };
        assert!(pos(products::FERRUMDATA) < sep);
        assert!(pos(products::FERRUMVARIABLES) > sep);
        assert!(!html.contains(products::FERRUM_LAUNCHER.tagline));
    }
}
