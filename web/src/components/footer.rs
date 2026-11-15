//! Site footer: the brand, the licence line and three link columns
//! (Product, Learn, Community). Classes: `.site-footer`, `.footer-credit`,
//! `.footer-grid`, `.footer-col`, `.footer-bottom`.

use maud::{Markup, html};

use crate::components::{
    button::{self, Variant},
    ferrum_mark,
};
use crate::pages::docs;
use crate::{site, updates};

pub fn render() -> Markup {
    html! {
        footer class="site-footer" {
            div class="container" {
                div class="footer-credit" {
                    p class="footer-credit__line" {
                        "Built in the open by "
                        a href=(site::AMYGDAL_URL) rel="noopener" { "amygdal.com" }
                        " for the Rust community."
                    }
                    (button::link(site::DOWNLOAD_LABEL, site::DOWNLOAD_PATH, Variant::Primary))
                }
                div class="footer-grid" {
                    div class="footer-col footer-col--brand" {
                        a class="brand" href="/" {
                            (ferrum_mark("brand__logo", ""))
                            span class="brand__name" { (site::SITE_NAME) }
                        }
                        p class="muted" { (site::SITE_TAGLINE) }
                        p class="footer-licence" {
                            "Open source under "
                            a href={ (crate::pages::licenses::PATH) "#ferrum" } { "MIT OR Apache-2.0" }
                            "."
                        }
                    }
                    nav class="footer-col" aria-labelledby="footer-product" {
                        h2 class="footer-col__title" id="footer-product" { "Product" }
                        ul {
                            li { a href=(site::DOWNLOAD_PATH) { "Download" } }
                            li { a href=(crate::pages::apps::PATH) { "All apps" } }
                            li { a href="/features" { "Features" } }
                            li { a href="/compare" { "Compare" } }
                            li { a href="/changelog" { "Changelog" } }
                            li { a href=(updates::INDEX_PATH) { "Updates" } }
                            li { a href=(updates::FEED_PATH) { "Atom feed" } }
                        }
                    }
                    nav class="footer-col" aria-labelledby="footer-learn" {
                        h2 class="footer-col__title" id="footer-learn" { "Learn" }
                        ul {
                            li { a href=(docs::PATH) { "All guides" } }
                            @for d in &docs::DOCS {
                                li { a href=(d.path) { (d.title) } }
                            }
                            li { a href="/faq" { "FAQ" } }
                            li { a href="/security" { "Security" } }
                            li { a href="/about" { "About" } }
                        }
                    }
                    nav class="footer-col" aria-labelledby="footer-community" {
                        h2 class="footer-col__title" id="footer-community" { "Community" }
                        ul {
                            li { a href=(site::SOURCE_REPO_URL) rel="noopener" { "Source on GitHub" } }
                            li { a href=(site::CONTRIBUTING_URL) rel="noopener" { "Contributing" } }
                            li { a href=(site::CODE_OF_CONDUCT_URL) rel="noopener" { "Code of conduct" } }
                            li { a href=(site::ISSUES_URL) rel="noopener" { "Issues" } }
                            li { a href=(site::DISCUSSIONS_URL) rel="noopener" { "Discussions" } }
                            li { a href=(site::RELEASES_URL) rel="noopener" { "Releases" } }
                        }
                    }
                }
                div class="footer-bottom" {
                    p {
                        "Desktop apps built in Rust with "
                        a href=(site::GPUI_URL) rel="noopener" { "GPUI" }
                        ", from the open-source "
                        a href=(site::ZED_UPSTREAM_URL) rel="noopener" { "Zed editor" }
                        ". This site: Axum and maud."
                    }
                    p {
                        a href=(crate::pages::privacy::PATH) { "Privacy" }
                        " · "
                        a href=(crate::pages::licenses::PATH) { "Licences" }
                    }
                    p class="footer-copy" {
                        "© 2026 " (site::ORGANIZATION_LEGAL_NAME) ", " (site::ORGANIZATION_CITY)
                        ". Code under " (site::LICENSE_SPDX) "."
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
    fn credits_zed_and_links_download() {
        let html = render().into_string();
        assert!(html.contains(site::ZED_UPSTREAM_URL));
        assert!(html.contains(r#"href="/download""#));
        assert!(html.contains("© 2026 AMYGDAL d.o.o."));
        assert!(html.contains(site::AMYGDAL_URL));
        assert!(html.contains("for the Rust community"));
        assert!(html.contains(r#"href="/privacy""#));
        assert!(html.contains(r#"href="/licenses""#));
        assert!(html.contains(r#"href="/updates""#));
        assert!(html.contains(r#"href="/apps""#));
        for path in [
            "/docs",
            "/docs/install",
            "/changelog",
            "/faq",
            "/security",
            "/compare",
        ] {
            assert!(html.contains(&format!(r#"href="{path}""#)), "{path}");
        }
    }

    #[test]
    fn states_the_licence_and_links_the_community() {
        let html = render().into_string();
        assert!(html.contains("MIT OR Apache-2.0"));
        for url in [
            site::SOURCE_REPO_URL,
            site::ISSUES_URL,
            site::DISCUSSIONS_URL,
            site::CONTRIBUTING_URL,
            site::RELEASES_URL,
        ] {
            assert!(html.contains(&format!(r#"href="{url}""#)), "{url}");
        }
        for heading in ["Product", "Learn", "Community"] {
            assert!(html.contains(&format!(">{heading}</h2>")), "{heading}");
        }
    }
}
