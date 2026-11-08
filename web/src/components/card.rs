//! Product card, app logos, the periodic-table "element tile" motif and
//! numbered steps.
//!
//! Classes: `.card`, `.card--product`, `.app-logo`, `.element-tile` (+ `--lg`,
//! `--brand`), `.feature-list`, `.feature`, `.steps`, `.step`.

use maud::{Markup, html};

use crate::components::{badge, icons};
use crate::products::Product;
use crate::routes::assets;

/// Periodic-table style tile, e.g. `element_tile("Fe", 26, "Ferrum", "")`.
/// `number` is shown top-left like an atomic number; decorative only.
pub fn element_tile(symbol: &str, number: u8, caption: &str, extra_class: &str) -> Markup {
    html! {
        span class={ "element-tile " (extra_class) } aria-hidden="true" {
            span class="element-tile__num" { (number) }
            span class="element-tile__sym" { (symbol) }
            span class="element-tile__cap" { (caption) }
        }
    }
}

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
            p class="card__kind" { (p.symbol) " · " (p.kind) }
            p class="card__body" { (p.tagline) }
            a class="card__link" href=(p.path()) {
                "Learn more" span class="visually-hidden" { " about " (p.name) }
                (icons::arrow())
            }
        }
    }
}

/// One feature tile: icon + title + text. Wrap several in `ul.feature-list`.
pub fn feature(icon: Markup, title: &str, text: &str) -> Markup {
    html! {
        li class="feature" {
            span class="feature__icon" { (icon) }
            h3 class="feature__title" { (title) }
            p class="feature__text" { (text) }
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
    fn steps_are_an_ordered_list() {
        let html = steps(&[("One", html! { "first" }), ("Two", html! { "second" })]).into_string();
        assert!(html.starts_with("<ol class=\"steps\">"));
        assert_eq!(html.matches("class=\"step\"").count(), 2);
    }
}
