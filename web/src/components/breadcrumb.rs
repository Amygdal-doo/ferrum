//! Breadcrumb trail. Classes: `.breadcrumb`, `.breadcrumb__list`.
//!
//! A trail is the [`Crumb`]s after Home; the last one is the current page.
//! The same crumbs feed the `BreadcrumbList` JSON-LD in `layout::page`.

use maud::{Markup, html};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crumb {
    pub name: String,
    /// Absolute path, e.g. `/docs/install`.
    pub path: String,
}

impl Crumb {
    pub fn new(name: &str, path: &str) -> Self {
        Self {
            name: name.to_owned(),
            path: path.to_owned(),
        }
    }
}

/// `Home / Docs / Install`, with the current page marked `aria-current`.
/// Renders nothing for an empty trail.
pub fn render(trail: &[Crumb]) -> Markup {
    if trail.is_empty() {
        return html! {};
    }
    let last = trail.len() - 1;
    html! {
        nav class="breadcrumb" aria-label="Breadcrumb" {
            ol class="breadcrumb__list" {
                li { a href="/" { "Home" } }
                @for (i, c) in trail.iter().enumerate() {
                    li {
                        @if i == last {
                            a href=(c.path) aria-current="page" { (c.name) }
                        } @else {
                            a href=(c.path) { (c.name) }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_the_current_page() {
        let html = render(&[
            Crumb::new("Docs", "/docs"),
            Crumb::new("Install", "/docs/install"),
        ])
        .into_string();
        assert!(html.contains(r#"aria-label="Breadcrumb""#));
        assert!(html.contains(r#"<a href="/">Home</a>"#));
        assert!(html.contains(r#"href="/docs/install" aria-current="page""#));
        assert_eq!(html.matches("aria-current").count(), 1);
        assert!(render(&[]).into_string().is_empty());
    }
}
