//! Structured data (JSON-LD) for search engines.
//!
//! Builders return `serde_json::Value`s; [`script`] embeds one in a page.
//! Claims follow the site: every build is a free download for macOS and
//! Linux, published by AMYGDAL d.o.o. in Sarajevo, and the source is open
//! under MIT OR Apache-2.0.

use maud::{Markup, PreEscaped, html};
use serde_json::{Value, json};

use crate::components::breadcrumb::Crumb;
use crate::products::Product;
use crate::site;

const CONTEXT: &str = "https://schema.org";

/// `<script type="application/ld+json">` with `</` escaped, so no string
/// inside the JSON can close the script element.
pub fn script(value: &Value) -> Markup {
    let json = value.to_string().replace("</", "<\\/");
    html! {
        script type="application/ld+json" { (PreEscaped(json)) }
    }
}

fn organization_id(site_url: &str) -> String {
    format!("{site_url}/#organization")
}

/// The company behind Ferrum.
pub fn organization(site_url: &str) -> Value {
    json!({
        "@context": CONTEXT,
        "@type": "Organization",
        "@id": organization_id(site_url),
        "name": site::ORGANIZATION_LEGAL_NAME,
        "alternateName": site::ORGANIZATION_NAME,
        "url": site::AMYGDAL_URL,
        "address": {
            "@type": "PostalAddress",
            "addressLocality": site::ORGANIZATION_CITY,
            "addressCountry": site::ORGANIZATION_COUNTRY,
        },
        "brand": { "@type": "Brand", "name": site::SITE_NAME },
    })
}

/// This website.
pub fn website(site_url: &str) -> Value {
    json!({
        "@context": CONTEXT,
        "@type": "WebSite",
        "@id": format!("{site_url}/#website"),
        "name": site::SITE_NAME,
        "description": site::SITE_DESCRIPTION,
        "url": format!("{site_url}/"),
        "inLanguage": "en",
        "publisher": { "@id": organization_id(site_url) },
    })
}

/// Home, then every crumb (the last one is the current page).
pub fn breadcrumb_list(site_url: &str, crumbs: &[Crumb]) -> Value {
    let home = Crumb::new("Home", "/");
    let items: Vec<Value> = std::iter::once(&home)
        .chain(crumbs)
        .enumerate()
        .map(|(i, c)| {
            json!({
                "@type": "ListItem",
                "position": i + 1,
                "name": c.name,
                "item": format!("{site_url}{}", c.path),
            })
        })
        .collect();
    json!({ "@context": CONTEXT, "@type": "BreadcrumbList", "itemListElement": items })
}

/// SPDX pages of the two licences Ferrum is offered under (either applies).
const LICENSE_URLS: [&str; 2] = [
    "https://spdx.org/licenses/MIT.html",
    "https://spdx.org/licenses/Apache-2.0.html",
];

/// The public source of every app: `codeRepository` lives on
/// `SoftwareSourceCode`, linked from the app with `isBasedOn`.
pub fn source_code() -> Value {
    json!({
        "@type": "SoftwareSourceCode",
        "name": "Ferrum",
        "codeRepository": site::SOURCE_REPO_URL,
        "programmingLanguage": "Rust",
        "license": LICENSE_URLS,
    })
}

/// One Ferrum app. `price: 0` because every build is a free download.
pub fn software_application(
    site_url: &str,
    product: &Product,
    description: &str,
    image: &str,
) -> Value {
    json!({
        "@context": CONTEXT,
        "@type": "SoftwareApplication",
        "name": product.name,
        "description": description,
        "url": format!("{site_url}{}", product.path()),
        "image": image,
        "applicationCategory": "DeveloperApplication",
        "applicationSubCategory": product.kind,
        "operatingSystem": "macOS, Linux",
        "downloadUrl": format!("{site_url}{}", site::DOWNLOAD_PATH),
        "isAccessibleForFree": true,
        "license": LICENSE_URLS,
        "isBasedOn": source_code(),
        "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" },
        "publisher": { "@id": organization_id(site_url) },
    })
}

/// Questions and plain-text answers.
pub fn faq_page(items: &[(&str, String)]) -> Value {
    let questions: Vec<Value> = items
        .iter()
        .map(|(q, a)| {
            json!({
                "@type": "Question",
                "name": q,
                "acceptedAnswer": { "@type": "Answer", "text": a },
            })
        })
        .collect();
    json!({ "@context": CONTEXT, "@type": "FAQPage", "mainEntity": questions })
}

/// A post on `/updates`.
pub fn blog_posting(
    site_url: &str,
    path: &str,
    headline: &str,
    summary: &str,
    date: &str,
    byline: &str,
) -> Value {
    json!({
        "@context": CONTEXT,
        "@type": "BlogPosting",
        "headline": headline,
        "description": summary,
        "datePublished": date,
        "url": format!("{site_url}{path}"),
        "mainEntityOfPage": format!("{site_url}{path}"),
        "author": { "@type": "Organization", "name": byline },
        "publisher": { "@id": organization_id(site_url) },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::products;

    #[test]
    fn script_cannot_be_closed_from_inside() {
        let html = script(&json!({ "x": "</script><b>" })).into_string();
        assert!(!html.contains("</script><b>"));
        assert!(html.contains(r"<\/script>"));
        assert!(html.ends_with("</script>"));
    }

    #[test]
    fn breadcrumbs_start_at_home() {
        let v = breadcrumb_list(
            "https://f.test",
            &[
                Crumb::new("Docs", "/docs"),
                Crumb::new("Install", "/docs/install"),
            ],
        );
        let items = v["itemListElement"].as_array().cloned().unwrap_or_default();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["item"], "https://f.test/");
        assert_eq!(items[2]["position"], 3);
    }

    #[test]
    fn app_is_a_free_macos_developer_application() {
        let v = software_application("https://f.test", &products::FERRUMCONTAINERS, "d", "i.png");
        assert_eq!(v["@type"], "SoftwareApplication");
        assert_eq!(v["operatingSystem"], "macOS, Linux");
        assert_eq!(v["license"][0], "https://spdx.org/licenses/MIT.html");
        assert_eq!(v["license"][1], "https://spdx.org/licenses/Apache-2.0.html");
        assert_eq!(v["isBasedOn"]["@type"], "SoftwareSourceCode");
        assert_eq!(
            v["isBasedOn"]["codeRepository"],
            "https://github.com/Amygdal-doo/ferrum"
        );
        assert_eq!(v["applicationCategory"], "DeveloperApplication");
        assert_eq!(v["offers"]["price"], "0");
        assert_eq!(v["publisher"]["@id"], "https://f.test/#organization");
    }

    #[test]
    fn organization_is_amygdal_in_sarajevo() {
        let v = organization("https://f.test");
        assert_eq!(v["name"], "AMYGDAL d.o.o.");
        assert_eq!(v["address"]["addressLocality"], "Sarajevo");
    }
}
