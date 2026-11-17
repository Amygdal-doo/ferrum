//! The app page's release notes, rendered as markdown: the version picked
//! in the list, else the latest release and the installed one (one block
//! when they are the same version).

use crate::app::FerrumLauncher;
use ferrum_ui::prelude::*;
use ferrumlauncher_core::{InstalledVersion, ReleaseInfo};
use semver::Version;

/// One block of notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesSection {
    pub version: Version,
    /// "Latest", "Installed", "Installed · latest" or "" for a picked row.
    pub role: &'static str,
    pub title: String,
    pub notes: String,
    /// The release page on GitHub.
    pub link: Option<String>,
}

impl NotesSection {
    fn from_release(release: &ReleaseInfo, role: &'static str) -> Self {
        Self {
            version: release.version.clone(),
            role,
            title: release.title.clone(),
            notes: release.notes.clone(),
            link: Some(release.html_url.clone()).filter(|url| !url.is_empty()),
        }
    }

    fn from_installed(installed: &InstalledVersion, role: &'static str) -> Self {
        Self {
            version: installed.version.clone(),
            role,
            title: String::new(),
            notes: installed.notes.clone(),
            link: None,
        }
    }
}

/// What the notes pane shows. `picked` is the version clicked in the list;
/// `releases` are the app's published releases; `installed` the active
/// version's record. A release still listed is preferred over the notes
/// saved at install time (they may have been edited since).
#[must_use]
pub fn sections(
    picked: Option<&Version>,
    releases: &[ReleaseInfo],
    latest: Option<&ReleaseInfo>,
    installed: Option<&InstalledVersion>,
) -> Vec<NotesSection> {
    let release_of = |version: &Version| releases.iter().find(|r| &r.version == version);
    if let Some(version) = picked {
        return match (release_of(version), installed) {
            (Some(release), _) => vec![NotesSection::from_release(release, "")],
            (None, Some(installed)) if &installed.version == version => {
                vec![NotesSection::from_installed(installed, "")]
            }
            _ => Vec::new(),
        };
    }
    let mut out = Vec::new();
    match (latest, installed) {
        (Some(latest), Some(installed)) if latest.version == installed.version => {
            out.push(NotesSection::from_release(latest, "Installed · latest"));
        }
        (latest, installed) => {
            if let Some(latest) = latest {
                out.push(NotesSection::from_release(latest, "Latest"));
            }
            if let Some(installed) = installed {
                out.push(match release_of(&installed.version) {
                    Some(release) => NotesSection::from_release(release, "Installed"),
                    None => NotesSection::from_installed(installed, "Installed"),
                });
            }
        }
    }
    out
}

impl FerrumLauncher {
    pub(crate) fn render_notes(&self, sections: Vec<NotesSection>, cx: &App) -> AnyElement {
        if sections.is_empty() {
            return div()
                .px_3()
                .child(
                    Label::new("No release selected.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element();
        }
        v_flex()
            .gap_4()
            .px_3()
            .pb_4()
            .children(sections.into_iter().enumerate().map(|(i, section)| {
                let heading = match (section.role, section.title.trim()) {
                    ("", "") => format!("v{}", section.version),
                    ("", title) => format!("v{} — {title}", section.version),
                    (role, "") => format!("{role}: v{}", section.version),
                    (role, title) => format!("{role}: v{} — {title}", section.version),
                };
                let body = if section.notes.trim().is_empty() {
                    Label::new("No release notes.")
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .into_any_element()
                } else {
                    ferrum_ui::markdown::render(format!("release-notes-{i}"), &section.notes, cx)
                };
                v_flex()
                    .gap_2()
                    .child(
                        Label::new(heading)
                            .size(LabelSize::Small)
                            .weight(ferrum_ui::gpui::FontWeight::SEMIBOLD),
                    )
                    .child(body)
                    .when_some(section.link, |this, url| {
                        this.child(
                            h_flex().child(
                                Button::new(
                                    SharedString::from(format!("open-release-{i}")),
                                    "Open on GitHub",
                                )
                                .style(ButtonStyle::Subtle)
                                .start_icon(Icon::new(IconName::ArrowUpRight))
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                            ),
                        )
                    })
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumlauncher_core::AppId;

    fn release(version: &str, notes: &str) -> ReleaseInfo {
        ReleaseInfo {
            app: AppId::GIT,
            version: Version::parse(version).expect("version"),
            tag: format!("git-v{version}"),
            title: format!("Title {version}"),
            notes: notes.into(),
            prerelease: false,
            published_at: None,
            html_url: format!("https://example.com/{version}"),
            assets: Vec::new(),
        }
    }

    fn installed(version: &str, notes: &str) -> InstalledVersion {
        InstalledVersion {
            version: Version::parse(version).expect("version"),
            tag: format!("git-v{version}"),
            asset: String::new(),
            size: 0,
            installed_at: chrono::Utc::now(),
            notes: notes.into(),
        }
    }

    #[test]
    fn latest_and_installed_are_both_shown() {
        let releases = [release("0.3.0", "## New\n- b"), release("0.2.0", "fresh")];
        let old = installed("0.2.0", "stale");
        let shown = sections(None, &releases, Some(&releases[0]), Some(&old));
        let summary: Vec<(&str, String, &str)> = shown
            .iter()
            .map(|s| (s.role, s.version.to_string(), s.notes.as_str()))
            .collect();
        // The installed version's notes come from its release while listed.
        assert_eq!(
            summary,
            [
                ("Latest", "0.3.0".to_string(), "## New\n- b"),
                ("Installed", "0.2.0".to_string(), "fresh"),
            ]
        );
    }

    #[test]
    fn one_block_when_up_to_date_and_saved_notes_when_the_release_is_gone() {
        let releases = [release("0.3.0", "notes")];
        let current = installed("0.3.0", "saved");
        let shown = sections(None, &releases, Some(&releases[0]), Some(&current));
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].role, "Installed · latest");

        let gone = installed("0.1.0", "saved");
        let shown = sections(None, &releases, Some(&releases[0]), Some(&gone));
        assert_eq!(shown[1].notes, "saved");
        assert_eq!(shown[1].link, None);

        let shown = sections(None, &[], None, Some(&gone));
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].role, "Installed");
    }

    #[test]
    fn a_picked_row_shows_only_that_version() {
        let releases = [release("0.3.0", "three"), release("0.2.0", "two")];
        let picked = Version::parse("0.2.0").expect("version");
        let shown = sections(Some(&picked), &releases, Some(&releases[0]), None);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].notes, "two");
        assert_eq!(shown[0].role, "");

        let removed = installed("0.1.0", "saved");
        let shown = sections(Some(&removed.version), &releases, None, Some(&removed));
        assert_eq!(shown[0].notes, "saved");
        let unknown = Version::parse("9.9.9").expect("version");
        assert!(sections(Some(&unknown), &releases, None, None).is_empty());
    }
}
