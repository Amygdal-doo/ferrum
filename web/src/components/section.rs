//! A page section: container, `<h2>` (always labelled) and an optional lead.
//!
//! Classes: `.section` (+ `--tight`, `--band`), `.section-header`, `.lead`.

use maud::{Markup, html};

pub struct Section<'a> {
    id: &'a str,
    title: &'a str,
    lead: Option<Markup>,
    tight: bool,
    band: bool,
    hide_title: bool,
}

impl<'a> Section<'a> {
    /// `id` names the heading (`{id}-title`) and is the section's anchor.
    pub fn new(id: &'a str, title: &'a str) -> Self {
        Self {
            id,
            title,
            lead: None,
            tight: false,
            band: false,
            hide_title: false,
        }
    }

    pub fn lead(mut self, lead: Markup) -> Self {
        self.lead = Some(lead);
        self
    }

    /// Less vertical padding.
    pub fn tight(mut self) -> Self {
        self.tight = true;
        self
    }

    /// Elevated background with rules above and below.
    pub fn band(mut self) -> Self {
        self.band = true;
        self
    }

    /// Keep the heading for screen readers and the outline, but hide it.
    pub fn hidden_title(mut self) -> Self {
        self.hide_title = true;
        self
    }

    pub fn render(self, body: Markup) -> Markup {
        let mut class = String::from("section");
        if self.tight {
            class.push_str(" section--tight");
        }
        if self.band {
            class.push_str(" section--band");
        }
        let title_id = format!("{}-title", self.id);
        html! {
            section class=(class) id=(self.id) aria-labelledby=(title_id) {
                div class="container" {
                    @if self.hide_title {
                        h2 id=(title_id) class="visually-hidden" { (self.title) }
                    } @else {
                        div class="section-header" {
                            h2 id=(title_id) { (self.title) }
                            @if let Some(lead) = &self.lead {
                                div class="lead" { (lead) }
                            }
                        }
                    }
                    (body)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_labels_the_section() {
        let html = Section::new("specs", "Specs")
            .band()
            .render(html! { p { "x" } })
            .into_string();
        assert!(
            html.contains(
                r#"class="section section--band" id="specs" aria-labelledby="specs-title""#
            )
        );
        assert!(html.contains(r#"<h2 id="specs-title">Specs</h2>"#));
    }
}
