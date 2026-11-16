//! `/stats`: how many installs use Ferrum, from FerrumLauncher's anonymous
//! daily count (`crate::usage`), plus GitHub's download totals.

use std::sync::Arc;

use maud::{Markup, html};

use crate::components::{
    breadcrumb::Crumb,
    callout::{self, Kind},
    page_header::PageHeader,
    section::Section,
};
use crate::layout::{PageMeta, page};
use crate::releases::DownloadTotals;
use crate::{products, usage::Aggregates};

pub const PATH: &str = "/stats";

/// What the usage half of the page can show.
#[derive(Debug, Clone)]
pub enum Usage {
    Counts(Arc<Aggregates>),
    /// The database is configured but can't be read right now.
    Unavailable,
    /// This copy of the site stores no counts (no `DATABASE_URL`).
    NotCollected,
}

/// `1234567` -> `1,234,567`.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn count(n: i64) -> String {
    thousands(u64::try_from(n).unwrap_or(0))
}

fn os_label(os: &str) -> &str {
    match os {
        "macos" => "macOS",
        "linux" => "Linux",
        "windows" => "Windows",
        other => other,
    }
}

fn arch_label(arch: &str) -> &str {
    match arch {
        "aarch64" => "ARM64",
        "x86_64" => "x86-64",
        other => other,
    }
}

fn app_name(id: &str) -> &str {
    products::by_release_id(id).map_or(id, |p| p.name)
}

pub fn render(site_url: &str, usage: &Usage, downloads: Option<&DownloadTotals>) -> Markup {
    let meta = PageMeta::new(
        Some("Usage stats: active installs and downloads"),
        "How many people use Ferrum: active installs per day, week and month from the \
         launcher's anonymous daily count, and GitHub download totals.",
        PATH,
        "page-stats",
    )
    .trail(vec![Crumb::new("Stats", PATH)]);
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, "Usage stats")
                .lead(html! {
                    p {
                        "Counted from FerrumLauncher's anonymous daily ping: one per install per \
                         UTC day, with no personal data. "
                        a href={ (super::privacy::PATH) "#usage-counts" } { "What is sent" } "."
                    }
                })
                .render())
            (Section::new("active", "Active installs").render(match usage {
                Usage::Counts(a) => active(a),
                Usage::Unavailable => callout::render(Kind::Note, Some("Counts unavailable"), html! {
                    p { "The counts can't be read right now. Try again in a few minutes." }
                }),
                Usage::NotCollected => callout::render(Kind::Note, Some("No counts yet"), html! {
                    p {
                        "This copy of the site isn't connected to a database, so it has no \
                         counts to show."
                    }
                }),
            }))
            @if let Usage::Counts(a) = usage {
                (Section::new("platforms", "Platforms").tight().render(platforms(a)))
                (Section::new("apps", "Installed apps").tight().render(apps(a)))
            }
            (Section::new("downloads", "Downloads").tight().band().render(downloads_table(downloads)))
        },
    )
}

fn active(a: &Aggregates) -> Markup {
    html! {
        dl class="hero-stats" {
            div class="hero-stat" { dt { "today (UTC, so far)" } dd { (count(a.today)) } }
            div class="hero-stat" { dt { "in the last 7 days" } dd { (count(a.last_7_days)) } }
            div class="hero-stat" { dt { "in the last 30 days" } dd { (count(a.last_30_days)) } }
        }
        p class="muted" {
            "Launchers with the count turned off aren't included, so real use is higher. \
             Figures refresh every few minutes."
        }
    }
}

fn platforms(a: &Aggregates) -> Markup {
    html! {
        div class="table-wrap" {
            table class="data-table" {
                caption class="visually-hidden" { "Active installs by platform, last 30 days" }
                thead { tr {
                    th scope="col" { "OS" }
                    th scope="col" { "CPU" }
                    th scope="col" { "Installs (30 days)" }
                } }
                tbody {
                    @for (os, arch, n) in &a.platforms {
                        tr {
                            th scope="row" { (os_label(os)) }
                            td { (arch_label(arch)) }
                            td { (count(*n)) }
                        }
                    }
                }
            }
        }
    }
}

fn apps(a: &Aggregates) -> Markup {
    html! {
        @if a.apps.is_empty() {
            p { "No apps reported yet." }
        } @else {
            div class="table-wrap" {
                table class="data-table" {
                    caption class="visually-hidden" { "Installs with each app, last 30 days" }
                    thead { tr {
                        th scope="col" { "App" }
                        th scope="col" { "Installs (30 days)" }
                    } }
                    tbody {
                        @for (id, n) in &a.apps {
                            tr { th scope="row" { (app_name(id)) } td { (count(*n)) } }
                        }
                    }
                }
            }
        }
    }
}

fn downloads_table(downloads: Option<&DownloadTotals>) -> Markup {
    let Some(d) = downloads else {
        return html! { p { "GitHub's download counts can't be loaded right now." } };
    };
    html! {
        p {
            strong { (thousands(d.total)) }
            " downloads of the newest 100 releases on GitHub (installers and launcher updates, \
             checksum files excluded)."
        }
        div class="table-wrap" {
            table class="data-table" {
                caption class="visually-hidden" { "GitHub downloads per app" }
                thead { tr { th scope="col" { "App" } th scope="col" { "Downloads" } } }
                tbody {
                    @for (id, n) in &d.by_app {
                        tr { th scope="row" { (app_name(id)) } td { (thousands(*n)) } }
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
    fn formats_thousands() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn empty_state_without_a_database() {
        let html = render("https://f.test", &Usage::NotCollected, None).into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains("No counts yet"));
        assert!(html.contains("/privacy#usage-counts"));
        assert!(!html.contains("hero-stats"));
    }

    #[test]
    fn shows_counts_platforms_apps_and_downloads() {
        let counts = Aggregates {
            today: 12,
            last_7_days: 1200,
            last_30_days: 3456,
            platforms: vec![("macos".into(), "aarch64".into(), 3000)],
            apps: vec![("git".into(), 800)],
        };
        let downloads = DownloadTotals {
            total: 9876,
            by_app: vec![("launcher".into(), 9876)],
        };
        let html = render(
            "https://f.test",
            &Usage::Counts(Arc::new(counts)),
            Some(&downloads),
        )
        .into_string();
        for text in [
            "1,200",
            "3,456",
            "macOS",
            "ARM64",
            "FerrumGIT",
            "9,876",
            "FerrumLauncher",
        ] {
            assert!(html.contains(text), "{text}");
        }
    }
}
