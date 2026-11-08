//! Status badge. Classes: `.badge`, `.badge--alpha`, `.badge--preview`.

use maud::{Markup, html};

use crate::products::Status;

/// Renders `<span class="badge badge--{css_class}">{label}</span>`.
pub fn status(status: Status) -> Markup {
    html! {
        span class={ "badge badge--" (status.css_class()) } { (status.label()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_modifier_and_label() {
        let html = status(Status::Alpha).into_string();
        assert!(html.contains("badge badge--alpha"));
        assert!(html.contains("Alpha"));
        assert!(
            status(Status::Preview)
                .into_string()
                .contains("Early preview")
        );
    }
}
