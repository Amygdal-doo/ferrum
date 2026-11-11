//! The header at the top of every inner page: breadcrumb, the one `<h1>`,
//! a lead paragraph, optional media (an app logo) and actions.
//!
//! Classes: `.page-header` (`--app` when tinted with an app accent),
//! `.page-header__inner`, `__copy`, `__title`, `__lead`, `__actions`, `__media`.
//! The temper rule along its bottom edge is the system's signature.

use maud::{Markup, html};

use super::breadcrumb::{self, Crumb};

pub struct PageHeader<'a> {
    pub trail: &'a [Crumb],
    pub title: &'a str,
    pub lead: Option<Markup>,
    pub media: Option<Markup>,
    pub actions: Option<Markup>,
    /// Inline `--app-accent` style from `Product::accent_style`.
    pub accent_style: Option<String>,
    /// Content placed right after the title (e.g. a status badge).
    pub title_suffix: Option<Markup>,
}

impl<'a> PageHeader<'a> {
    pub fn new(trail: &'a [Crumb], title: &'a str) -> Self {
        Self {
            trail,
            title,
            lead: None,
            media: None,
            actions: None,
            accent_style: None,
            title_suffix: None,
        }
    }

    pub fn lead(mut self, lead: Markup) -> Self {
        self.lead = Some(lead);
        self
    }

    pub fn media(mut self, media: Markup) -> Self {
        self.media = Some(media);
        self
    }

    pub fn actions(mut self, actions: Markup) -> Self {
        self.actions = Some(actions);
        self
    }

    pub fn accent(mut self, style: String) -> Self {
        self.accent_style = Some(style);
        self
    }

    pub fn title_suffix(mut self, suffix: Markup) -> Self {
        self.title_suffix = Some(suffix);
        self
    }

    pub fn render(&self) -> Markup {
        let class = if self.accent_style.is_some() {
            "page-header page-header--app"
        } else {
            "page-header"
        };
        html! {
            header class=(class) style=[self.accent_style.as_deref()] {
                div class="container page-header__inner" {
                    div class="page-header__copy" {
                        (breadcrumb::render(self.trail))
                        div class="page-header__title-row" {
                            h1 class="page-header__title" id="page-title" { (self.title) }
                            @if let Some(suffix) = &self.title_suffix { (suffix) }
                        }
                        @if let Some(lead) = &self.lead {
                            div class="page-header__lead" { (lead) }
                        }
                        @if let Some(actions) = &self.actions {
                            div class="page-header__actions" { (actions) }
                        }
                    }
                    @if let Some(media) = &self.media {
                        div class="page-header__media" { (media) }
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
    fn one_h1_with_trail_and_optional_parts() {
        let trail = [Crumb::new("FAQ", "/faq")];
        let html = PageHeader::new(&trail, "Questions")
            .lead(html! { p { "Answers." } })
            .render()
            .into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(r#"id="page-title""#));
        assert!(html.contains("Breadcrumb"));
        assert!(html.contains("Answers."));
        assert!(!html.contains("page-header__media"));
        assert!(!html.contains("page-header--app"));
    }
}
