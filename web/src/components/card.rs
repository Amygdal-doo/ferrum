//! The app card, app logos, the compact app strip and numbered steps.
//!
//! Classes: `.card`, `.card--product`, `.app-logo` (`--hero`, `--sm`, `--xs`),
//! `.app-strip`, `.steps`, `.step`.

use maud::{Markup, html};

use crate::components::{badge, icons};
use crate::products::Product;
use crate::routes::assets;

/// The app's logo as an `<img>`. Alt text names the app unless the logo sits
/// next to the app's name already (pass `decorative = true`).
pub fn app_logo(p: &Product, extra_class: &str, decorative: bool) -> Markup {
    let alt = if decorative {
        String::new()
    } else {
        format!("{} logo", p.name)
    };
    html! {
        img class={ "app-logo " (extra_class) } src=(assets::url(&p.logo_asset())) alt=(alt)
            width="64" height="64" decoding="async";
    }
}

/// Card for one product: logo, name, kind, status badge, tagline, "Learn more".
pub fn product_card(p: &Product) -> Markup {
    let heading_id = format!("card-{}", p.slug);
    html! {
        article class="card card--product" style=(p.accent_style()) aria-labelledby=(heading_id) {
            div class="card__top" {
                (app_logo(p, "", true))
                (badge::status(p.status))
            }
            h3 class="card__title" id=(heading_id) { (p.name) }
            p class="card__kind" { (p.kind) }
            p class="card__body" { (p.tagline) }
            a class="card__link" href=(p.path()) {
                "Learn more" span class="visually-hidden" { " about " (p.name) }
                (icons::arrow())
            }
        }
    }
}

/// Compact links to apps: logo and name, optionally the kind. Class
/// `.app-strip` (`--rows` stacks them with the kind shown).
pub fn app_strip<'a>(apps: impl IntoIterator<Item = &'a Product>, with_kind: bool) -> Markup {
    let class = if with_kind {
        "app-strip app-strip--rows"
    } else {
        "app-strip"
    };
    html! {
        ul class=(class) {
            @for p in apps {
                li style=(p.accent_style()) {
                    a class="app-strip__link" href=(p.path()) {
                        (app_logo(p, "app-logo--xs", true))
                        span class="app-strip__name" { (p.name) }
                        @if with_kind { span class="app-strip__kind" { (p.kind) } }
                    }
                }
            }
        }
    }
}

/// Numbered how-to steps. Each body is trusted, authored markup.
pub fn steps(items: &[(&str, Markup)]) -> Markup {
    html! {
        ol class="steps" {
            @for (title, body) in items {
                li class="step" {
                    h3 class="step__title" { (title) }
                    div class="step__body" { (body) }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::products;

    #[test]
    fn card_links_to_detail_page_with_logo_and_accent() {
        let html = product_card(&products::FERRUMDATA).into_string();
        assert!(html.contains(r#"href="/ferrumdata""#));
        assert!(html.contains("FerrumData"));
        assert!(html.contains("badge--alpha"));
        assert!(html.contains("/assets/img/brand/data.svg?v="));
        assert!(html.contains("--app-accent: var(--app-data)"));
    }

    #[test]
    fn logo_alt_text_depends_on_context() {
        let named = app_logo(&products::FERRUMGIT, "", false).into_string();
        assert!(named.contains(r#"alt="FerrumGIT logo""#));
        let decorative = app_logo(&products::FERRUMGIT, "", true).into_string();
        assert!(decorative.contains(r#"alt="""#));
    }

    #[test]
    fn app_strip_links_each_app() {
        let html = app_strip(products::apps(), true).into_string();
        assert_eq!(
            html.matches("app-strip__link").count(),
            products::apps().count()
        );
        assert!(html.contains("app-strip--rows"));
        assert!(html.contains("Container manager"));
    }

    #[test]
    fn steps_are_an_ordered_list() {
        let html = steps(&[("One", html! { "first" }), ("Two", html! { "second" })]).into_string();
        assert!(html.starts_with("<ol class=\"steps\">"));
        assert_eq!(html.matches("class=\"step\"").count(), 2);
    }
}
