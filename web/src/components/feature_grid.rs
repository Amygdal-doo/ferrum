//! A grid of (title, text) features. Classes: `.feature-list`, `.feature`,
//! `.feature__title`, `.feature__text`. Titles are `<h3>`, so place the grid
//! under an `<h2>`.

use maud::{Markup, html};

pub fn render(items: &[(&str, &str)]) -> Markup {
    html! {
        ul class="feature-list" {
            @for (title, text) in items {
                li class="feature" {
                    h3 class="feature__title" { (title) }
                    p class="feature__text" { (text) }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_item_per_feature() {
        let html = render(&[("A", "a"), ("B", "b")]).into_string();
        assert_eq!(html.matches(r#"class="feature""#).count(), 2);
        assert!(html.contains(r#"<h3 class="feature__title">A</h3>"#));
    }
}
