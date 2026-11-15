//! `/changelog`: every release of every app, read from the public releases
//! repo (see `releases.rs`). When GitHub can't be reached and nothing is
//! cached, the page says so and links the releases page.

use maud::{Markup, html};

use crate::components::{
    breadcrumb::Crumb,
    button::{self, Variant},
    callout::{self, Kind},
    card,
    page_header::PageHeader,
};
use crate::layout::{PageMeta, page};
use crate::releases::ChangelogEntry;
use crate::{products, site, updates};

pub const PATH: &str = "/changelog";
/// Entries shown on the page; the rest are on GitHub.
const MAX_ENTRIES: usize = 60;
/// Note lines shown per release before "Full notes on GitHub".
const MAX_NOTE_LINES: usize = 12;

/// `2026-11` -> `November 2026`.
fn month_label(date: &str) -> String {
    let label = updates::display_date(&format!("{}-01", date.get(..7).unwrap_or(date)));
    label
        .split_once(' ')
        .map_or(label.clone(), |(_, rest)| rest.to_owned())
}

/// Strips Markdown emphasis and heading marks from one line of notes.
fn plain(line: &str) -> String {
    line.trim_start_matches('#')
        .trim()
        .replace("**", "")
        .replace("__", "")
}

/// Release notes as paragraphs and lists. Notes are untrusted text: maud
/// escapes everything, and only line structure is interpreted.
fn notes(text: &str) -> Markup {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let truncated = lines.len() > MAX_NOTE_LINES;
    let mut blocks: Vec<(bool, Vec<String>)> = Vec::new();
    for line in lines.iter().take(MAX_NOTE_LINES) {
        let t = line.trim();
        let item = t.strip_prefix("- ").or_else(|| t.strip_prefix("* "));
        match (item, blocks.last_mut()) {
            (Some(i), Some((true, items))) => items.push(plain(i)),
            (Some(i), _) => blocks.push((true, vec![plain(i)])),
            (None, _) => blocks.push((false, vec![plain(t)])),
        }
    }
    html! {
        @for (is_list, items) in &blocks {
            @if *is_list {
                ul class="prose-list" { @for i in items { li { (i) } } }
            } @else {
                @for p in items { p { (p) } }
            }
        }
        @if truncated { p class="muted" { "Notes shortened." } }
    }
}

fn entry(e: &ChangelogEntry) -> Markup {
    let Some(p) = products::by_release_id(&e.app_id) else {
        return html! {};
    };
    let id = format!("{}-{}", e.app_id, e.version.replace('.', "-"));
    html! {
        article class="release" id=(id) style=(p.accent_style()) aria-labelledby={ (id) "-title" } {
            div class="release__meta" {
                (card::app_logo(p, "app-logo--xs", true))
                time datetime=(e.date) { (updates::display_date(&e.date)) }
                span class={ "chip chip--" (e.channel.label().to_lowercase()) } { (e.channel.label()) }
            }
            h3 class="release__title" id={ (id) "-title" } {
                a href=(p.path()) { (p.name) } " " (e.version)
            }
            @if e.notes.is_empty() {
                p class="muted" { "No release notes." }
            } @else {
                div class="release__notes" { (notes(&e.notes)) }
            }
            p class="release__link" {
                a href=(e.url) rel="noopener" {
                    "Release on GitHub"
                    span class="visually-hidden" { ": " (p.name) " " (e.version) }
                }
            }
        }
    }
}

fn timeline(entries: &[ChangelogEntry]) -> Markup {
    let shown: Vec<&ChangelogEntry> = entries
        .iter()
        .filter(|e| products::by_release_id(&e.app_id).is_some())
        .take(MAX_ENTRIES)
        .collect();
    let mut months: Vec<(String, Vec<&ChangelogEntry>)> = Vec::new();
    for e in shown {
        let m = e.date.get(..7).unwrap_or_default().to_owned();
        match months.last_mut() {
            Some((last, list)) if *last == m => list.push(e),
            _ => months.push((m, vec![e])),
        }
    }
    html! {
        @if months.is_empty() {
            p { "No releases published yet." }
        }
        @for (month, list) in &months {
            section class="changelog-month" aria-labelledby={ "m-" (month) } {
                h2 id={ "m-" (month) } { (month_label(month)) }
                @for e in list { (entry(e)) }
            }
        }
    }
}

pub fn render(site_url: &str, entries: Option<&[ChangelogEntry]>) -> Markup {
    let meta = PageMeta::new(
        Some("Changelog: every Ferrum release"),
        "Every release of every Ferrum app, newest first, with its notes: FerrumIDE, \
         FerrumAPI, FerrumGIT, FerrumData, FerrumTerm and the rest.",
        PATH,
        "page-changelog",
    )
    .trail(vec![Crumb::new("Changelog", PATH)]);
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, "Changelog")
                .lead(html! {
                    p {
                        "Every release of every app, newest first, with its notes. Ferrum \
                         Launcher offers the same releases as updates. Announcements are on "
                        a href=(updates::INDEX_PATH) { "Updates" } "."
                    }
                })
                .render())
            div class="container changelog" {
                @match entries {
                    Some(list) => (timeline(list)),
                    None => (callout::render(Kind::Warn, Some("Releases can't be loaded right now"), html! {
                        p {
                            "GitHub didn't answer. Every release is listed in the public "
                            code { (site::REPO_SLUG) } " repository."
                        }
                    })),
                }
                p class="changelog__all" {
                    (button::link("All releases on GitHub", site::RELEASES_URL, Variant::Secondary))
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::releases::{Channel, changelog};

    #[test]
    fn groups_by_month_and_escapes_notes() {
        let json = r###"[
          {"tag_name": "api-v0.2.0", "published_at": "2026-11-02T09:00:00Z",
           "body": "## Fixed\n- <script>alert(1)</script>\n- **Bold** item", "html_url": "https://gh.test/a"},
          {"tag_name": "ide-v1.0.0", "published_at": "2026-10-20T09:00:00Z", "body": "gone"},
          {"tag_name": "git-v0.1.0-alpha.1", "published_at": "2026-10-03T09:00:00Z", "prerelease": true}
        ]"###;
        let entries = changelog(json).unwrap_or_default();
        assert_eq!(entries[2].channel, Channel::Alpha);
        let html = render("https://f.test", Some(&entries)).into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(">November 2026</h2>"));
        assert!(html.contains(">October 2026</h2>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("<li>Bold item</li>"));
        assert!(!html.contains("Ferrum IDE"), "retired apps are left out");
        assert!(html.contains("No release notes."));
    }

    #[test]
    fn says_so_when_offline() {
        let html = render("https://f.test", None).into_string();
        assert!(html.contains("can't be loaded"));
        assert!(html.contains(site::RELEASES_URL));
    }

    #[test]
    fn labels_months() {
        assert_eq!(month_label("2026-11"), "November 2026");
        assert_eq!(month_label("2026-01-15"), "January 2026");
    }
}
