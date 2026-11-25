//! The shared search modal: apps and tools by name (⌘P, ⌘F and Shift
//! twice), their releases as symbols, and text in descriptions and release
//! notes (⌘⇧F). A result opens the app's page (on that release).

use std::sync::Arc;
use std::sync::atomic::Ordering;

use ferrum_ui::prelude::*;
use ferrum_ui::search::{
    SearchCategory, SearchEvent, SearchItem, SearchModal, SearchMode, TextMatcher, TextSearcher,
};
use ferrumlauncher_core::{AppId, AppInfo, ReleaseInfo};

use crate::app::FerrumLauncher;

/// One searchable text: where it opens, what it is called, the text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Doc {
    pub id: String,
    pub label: String,
    pub text: String,
}

fn release_id(r: &ReleaseInfo) -> String {
    format!("release:{}:{}", r.app, r.version)
}

/// Apps and tools as items; their releases as symbols.
pub(crate) fn items(apps: &[(AppInfo, Vec<&ReleaseInfo>)]) -> Vec<SearchItem> {
    let mut items = Vec::new();
    for (app, _) in apps {
        items.push(
            SearchItem::new(
                SearchCategory::Items,
                app.name.clone(),
                format!("app:{}", app.id),
            )
            .detail(app.description.clone())
            .icon(IconName::Box),
        );
    }
    for (app, releases) in apps {
        for r in releases {
            items.push(
                SearchItem::new(
                    SearchCategory::Symbols,
                    format!("{} {}", app.name, r.version),
                    release_id(r),
                )
                .detail(if r.is_prerelease() {
                    "Alpha release"
                } else {
                    "Release"
                })
                .icon(IconName::Bookmark),
            );
        }
    }
    items
}

/// Descriptions and release notes, one [`Doc`] each.
pub(crate) fn docs(apps: &[(AppInfo, Vec<&ReleaseInfo>)]) -> Vec<Doc> {
    let mut docs = Vec::new();
    for (app, releases) in apps {
        docs.push(Doc {
            id: format!("app:{}", app.id),
            label: app.name.clone(),
            text: format!("{}\n{}", app.name, app.description),
        });
        for r in releases {
            docs.push(Doc {
                id: release_id(r),
                label: format!("{} {}", app.name, r.version),
                text: format!("{}\n{}", r.title, r.notes),
            });
        }
    }
    docs
}

/// Text search over [`docs`]; the mask matches their labels.
pub(crate) fn searcher(docs: Vec<Doc>) -> TextSearcher {
    Arc::new(move |query, cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let mut hits = Vec::new();
        for d in &docs {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if matcher.allows(&d.label)
                && !matcher.find_lines(&d.text, &d.id, &d.label, limit, &mut hits)
            {
                break;
            }
        }
        Ok(hits)
    })
}

impl FerrumLauncher {
    fn search_apps(&self) -> Vec<(AppInfo, Vec<&ReleaseInfo>)> {
        self.state
            .catalog()
            .apps()
            .into_iter()
            .map(|app| {
                let releases = self.state.releases_for(app.id);
                (app, releases)
            })
            .collect()
    }

    pub(crate) fn toggle_search(
        &mut self,
        mode: SearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.search.take() {
            self.search_sub = None;
            if search.read(cx).mode() == mode {
                window.focus(&self.focus_handle, cx);
                cx.notify();
                return;
            }
        }
        let apps = self.search_apps();
        let items = match mode {
            SearchMode::FindInFiles => Vec::new(),
            SearchMode::GoTo => items(&apps)
                .into_iter()
                .filter(|i| i.category == SearchCategory::Items)
                .collect(),
            SearchMode::Everywhere => items(&apps),
        };
        let searcher = (mode != SearchMode::GoTo).then(|| searcher(docs(&apps)));
        let search = cx.new(|cx| SearchModal::new(mode, "Apps", items, searcher, window, cx));
        self.search_sub = Some(cx.subscribe_in(&search, window, Self::on_search_event));
        self.search = Some(search);
        cx.notify();
    }

    /// Closes the modal; true when one was open (Escape).
    pub(crate) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.search_sub = None;
        if self.search.take().is_none() {
            return false;
        }
        window.focus(&self.focus_handle, cx);
        cx.notify();
        true
    }

    fn on_search_event(
        &mut self,
        _: &Entity<SearchModal>,
        event: &SearchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_search(window, cx);
        let id = match event {
            SearchEvent::Open(item) => item.id.to_string(),
            SearchEvent::OpenHit(hit) => hit.source.to_string(),
            SearchEvent::Dismissed => return,
        };
        self.open_search_target(&id, cx);
    }

    /// Opens the app's page a result points at, on its release if any.
    pub(crate) fn open_search_target(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some((kind, rest)) = id.split_once(':') else {
            return;
        };
        let (app, version) = match kind {
            "release" => match rest.split_once(':') {
                Some((app, version)) => (app, semver::Version::parse(version).ok()),
                None => return,
            },
            _ => (rest, None),
        };
        let Ok(app) = app.parse::<AppId>() else {
            return;
        };
        self.select_app(app, cx);
        if version.is_some() {
            self.selected_release = version;
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_ui::search::TextQuery;
    use std::sync::atomic::AtomicBool;

    fn app(id: AppId, name: &str, description: &str) -> AppInfo {
        let mut info = ferrumlauncher_core::Catalog::default()
            .info(id)
            .expect("built-in app");
        info.name = name.into();
        info.description = description.into();
        info
    }

    fn release(app: AppId, version: &str, notes: &str) -> ReleaseInfo {
        ReleaseInfo {
            app,
            version: semver::Version::parse(version).expect("version"),
            tag: format!("{app}-v{version}"),
            title: format!("{app} {version}"),
            notes: notes.into(),
            prerelease: version.contains('-'),
            published_at: None,
            html_url: String::new(),
            assets: Vec::new(),
        }
    }

    #[test]
    fn apps_are_items_and_releases_symbols() {
        let r = release(AppId::TERM, "0.18.0-alpha.1", "Agents talk");
        let apps = vec![(app(AppId::TERM, "FerrumTerm", "GPU terminal"), vec![&r])];
        let items = items(&apps);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].category, SearchCategory::Items);
        assert_eq!(items[0].id.as_ref(), "app:term");
        assert_eq!(items[1].category, SearchCategory::Symbols);
        assert_eq!(items[1].id.as_ref(), "release:term:0.18.0-alpha.1");
        assert_eq!(items[1].title.as_ref(), "FerrumTerm 0.18.0-alpha.1");
    }

    #[test]
    fn text_search_reads_descriptions_and_notes() {
        let r = release(
            AppId::TERM,
            "0.18.0-alpha.1",
            "Shared context\nClick to move",
        );
        let apps = vec![(app(AppId::TERM, "FerrumTerm", "GPU terminal"), vec![&r])];
        let search = searcher(docs(&apps));
        let hits = search(
            &TextQuery {
                text: "click".into(),
                ..TextQuery::default()
            },
            &AtomicBool::new(false),
            10,
        )
        .expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source.as_ref(), "release:term:0.18.0-alpha.1");
        assert_eq!(hits[0].line, 2);
        let hits = search(
            &TextQuery {
                text: "gpu".into(),
                ..TextQuery::default()
            },
            &AtomicBool::new(false),
            10,
        )
        .expect("search");
        assert_eq!(hits[0].source.as_ref(), "app:term");
    }
}
