//! Collections grouped by project: the current project, shared, and (with
//! no project open) every other project.

use crate::app::FerrumApi;
use crate::components::hint;
use crate::components::shell::project::project_name;
use ferrum_ui::components::{
    Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _, Color, Label,
    LabelCommon as _, LabelSize, Tooltip, h_flex,
};
use ferrumapi_domain::Collection;
use gpui::{AnyElement, Context, prelude::*};

#[derive(Debug, Clone, PartialEq)]
pub enum GroupKind {
    /// The open project (path).
    Current(String),
    Shared,
    /// Another project (path), listed when no project is open.
    Other(String),
    /// Collections of other projects hidden while a project is open.
    Hidden(usize),
}

#[derive(Debug, Clone)]
pub struct Group {
    pub kind: GroupKind,
    pub collections: Vec<Collection>,
}

/// Groups `list` (already filtered) for the sidebar.
#[must_use]
pub fn group_collections(list: Vec<Collection>, project: Option<&str>) -> Vec<Group> {
    let mut current = Vec::new();
    let mut shared = Vec::new();
    let mut others: Vec<(String, Vec<Collection>)> = Vec::new();
    for c in list {
        match c.project.clone() {
            None => shared.push(c),
            Some(p) if Some(p.as_str()) == project => current.push(c),
            Some(p) => match others.iter_mut().find(|(path, _)| *path == p) {
                Some((_, v)) => v.push(c),
                None => others.push((p, vec![c])),
            },
        }
    }
    let mut out = Vec::new();
    if let Some(p) = project {
        out.push(Group {
            kind: GroupKind::Current(p.to_owned()),
            collections: current,
        });
    }
    out.push(Group {
        kind: GroupKind::Shared,
        collections: shared,
    });
    if project.is_some() {
        let hidden: usize = others.iter().map(|(_, v)| v.len()).sum();
        if hidden > 0 {
            out.push(Group {
                kind: GroupKind::Hidden(hidden),
                collections: Vec::new(),
            });
        }
    } else {
        others.sort_by_key(|(p, _)| project_name(p).to_lowercase());
        out.extend(others.into_iter().map(|(p, collections)| Group {
            kind: GroupKind::Other(p),
            collections,
        }));
    }
    out
}

impl FerrumApi {
    /// Header row of a group; `None` for the shared group when it is the
    /// only one (no projects in use).
    pub(super) fn render_group_header(
        &mut self,
        group: &Group,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let title = |text: String| {
            Label::new(text.to_uppercase())
                .size(LabelSize::XSmall)
                .color(Color::Muted)
        };
        let row = h_flex().pt_2().pb_1().px_1().gap_1();
        Some(match &group.kind {
            GroupKind::Current(path) => row
                .child(title(format!("Project · {}", project_name(path))).into_any_element())
                .child(gpui::div().flex_1())
                .child(
                    Button::new("project-save-repo", "Save to repo")
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .label_size(LabelSize::XSmall)
                        .tooltip(Tooltip::text(
                            "Write this project's collections to .ferrum/api/*.json (secret variable values are left out)",
                        ))
                        .on_click(cx.listener(|this, _, _, cx| this.export_project_to_repo(cx))),
                )
                .child(
                    Button::new("project-load-repo", "Load")
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .label_size(LabelSize::XSmall)
                        .tooltip(Tooltip::text("Load collections from .ferrum/api/*.json"))
                        .on_click(cx.listener(|this, _, _, cx| this.import_project_from_repo(cx))),
                )
                .child(
                    Button::new("project-close", "Close")
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .label_size(LabelSize::XSmall)
                        .tooltip(Tooltip::text("Leave the project (show shared collections only)"))
                        .on_click(cx.listener(|this, _, _, cx| this.open_project(None, cx))),
                )
                .into_any_element(),
            GroupKind::Shared if group.collections.is_empty() => return None,
            GroupKind::Shared => {
                let solo = self.state.project.is_none()
                    && !self.state.collections.iter().any(|c| c.project.is_some());
                if solo {
                    return None;
                }
                row.child(title("No project / shared".into())).into_any_element()
            }
            GroupKind::Other(path) => row
                .child(title(project_name(path)))
                .into_any_element(),
            GroupKind::Hidden(n) => row
                .child(hint(format!(
                    "{n} collection(s) in other projects: switch project to see them"
                )))
                .into_any_element(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str, project: Option<&str>) -> Collection {
        let mut c = Collection::new(name);
        c.project = project.map(str::to_owned);
        c
    }

    #[test]
    fn groups_follow_the_current_project() {
        let list = vec![
            named("a", Some("/p/app")),
            named("s", None),
            named("o", Some("/p/other")),
        ];
        let kinds: Vec<_> = group_collections(list.clone(), Some("/p/app"))
            .into_iter()
            .map(|g| (g.kind, g.collections.len()))
            .collect();
        assert_eq!(
            kinds,
            [
                (GroupKind::Current("/p/app".into()), 1),
                (GroupKind::Shared, 1),
                (GroupKind::Hidden(1), 0)
            ]
        );
        let kinds: Vec<_> = group_collections(list, None)
            .into_iter()
            .map(|g| (g.kind, g.collections.len()))
            .collect();
        assert_eq!(
            kinds,
            [
                (GroupKind::Shared, 1),
                (GroupKind::Other("/p/app".into()), 1),
                (GroupKind::Other("/p/other".into()), 1)
            ]
        );
    }
}
