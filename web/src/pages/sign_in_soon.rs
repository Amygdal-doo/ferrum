//! `/login` and `/signup` while the accounts service isn't connected
//! (`ACCOUNTS_URL` unset): sign-in is coming soon. Not indexed, not in the
//! sitemap.

use maud::{Markup, html};

use crate::components::{breadcrumb::Crumb, page_header::PageHeader};
use crate::layout::{PageMeta, page};

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Sign in"),
        "Ferrum accounts are coming soon. Every Ferrum app works without an account in \
         the meantime.",
        "/login",
        "page-account",
    )
    .noindex();
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&[Crumb::new("Sign in", "/login")], "Sign-in is coming soon")
                .lead(html! {
                    p {
                        "Ferrum accounts aren't open yet. Every app works without one: "
                        a href=(crate::site::DOWNLOAD_PATH) { "download FerrumLauncher" }
                        " and start using it."
                    }
                })
                .render())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_coming_soon_and_is_noindex() {
        let html = render("https://f.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains("Sign-in is coming soon"));
        assert!(html.contains(r#"<meta name="robots" content="noindex">"#));
        assert!(!html.contains("<form"));
    }
}
