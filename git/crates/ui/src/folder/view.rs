//! The folder pane: a header with the folder and "Reset", the Changes and
//! Repositories tabs, the changed files over the selected file's diff,
//! and the repositories below. Plus the strip above a picked repository
//! that leads back to the folder.

use super::scan::TooBig;
use super::{Baseline, FolderMode, FolderPage};
use crate::app::GitView;
use crate::components::code::code_diff;
use crate::components::detail::files::{FileRow, RowAction};
use crate::components::diff::first_line;
use crate::components::icons::Icon;
use crate::components::widgets::{ghost_button, hint, small_button};
use crate::components::{change_word, plural};
use ferrum_ui::components::{
    ButtonCommon, Clickable, Color, IconSize, Label, LabelCommon, LabelSize, StyledTypography,
    Tooltip, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{ChangeKind, DiffSource};
use gpui::{AnyElement, Context, ElementId, SharedString, div, prelude::*, px};
use std::path::{Path, PathBuf};

const OPEN: RowAction = RowAction {
    label: "Open",
    title: "Open this file in the editor",
    danger: false,
};

/// `~/code/app` for paths in the home folder.
fn shown_path(path: &Path) -> String {
    match super::home().and_then(|home| path.strip_prefix(&home).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| shown_path(path), |n| n.to_string_lossy().into_owned())
}

/// The whole pane (in place of the compact toolbar and body).
pub(crate) fn render(app: &GitView, cx: &mut Context<GitView>) -> AnyElement {
    let Some(folder) = app.folder.as_ref() else {
        return div().into_any_element();
    };
    let body = match folder.page {
        FolderPage::Changes => changes_page(app, folder, cx),
        FolderPage::Repositories => v_flex()
            .id("folder-repos-page")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(repo_list(folder, true, cx))
            .into_any_element(),
    };
    v_flex()
        .flex_1()
        .min_h_0()
        .child(header(folder, cx))
        .child(body)
        .into_any_element()
}

fn header(folder: &FolderMode, cx: &mut Context<GitView>) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let can_reset = matches!(folder.baseline, Baseline::Ready { .. });
    let tab = |page: FolderPage, label: &'static str, count: usize, cx: &mut Context<GitView>| {
        let active = folder.page == page;
        h_flex()
            .id(ElementId::Name(format!("folder-page-{label}").into()))
            .flex_none()
            .h_full()
            .gap_1()
            .px_2()
            .cursor_pointer()
            .border_b_2()
            .border_color(if active {
                colors.text_accent
            } else {
                gpui::transparent_black()
            })
            .hover(|s| s.bg(colors.element_hover))
            .child(Label::new(label).size(LabelSize::Small).color(if active {
                Color::Accent
            } else {
                Color::Muted
            }))
            .when(count > 0, |d| {
                d.child(
                    Label::new(count.to_string())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
            })
            .on_click(cx.listener(move |app, _, _, cx| app.set_folder_page(page, cx)))
    };
    let changes = tab(FolderPage::Changes, "Changes", folder.changes.len(), cx);
    let repos = folder.repos.as_ref().map_or(0, Vec::len);
    let repositories = tab(FolderPage::Repositories, "Repositories", repos, cx);
    v_flex()
        .flex_none()
        .border_b_1()
        .border_color(colors.border)
        .bg(colors.title_bar_background)
        .child(
            h_flex()
                .min_h(px(36.))
                .gap_1p5()
                .px_2p5()
                .child(
                    Icon::Folder
                        .icon()
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(folder_name(&folder.path))
                        .size(LabelSize::Small)
                        .weight(gpui::FontWeight::SEMIBOLD),
                )
                .child(
                    div().flex_1().min_w_0().child(
                        Label::new("no git")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    ),
                )
                .when(can_reset && folder.page == FolderPage::Changes, |row| {
                    row.child(
                        small_button("folder-reset", "Reset")
                            .tooltip(Tooltip::text("Take a new snapshot: changes count from now"))
                            .on_click(cx.listener(|app, _, _, cx| app.take_baseline(cx))),
                    )
                }),
        )
        .child(
            h_flex()
                .h(px(30.))
                .gap_1()
                .px_1p5()
                .border_t_1()
                .border_color(colors.border_variant)
                .child(changes)
                .child(repositories),
        )
        .into_any_element()
}

fn status_line(text: impl Into<SharedString>) -> AnyElement {
    div()
        .flex_none()
        .px_3()
        .py_2()
        .child(hint(text))
        .into_any_element()
}

fn changes_page(app: &GitView, folder: &FolderMode, cx: &mut Context<GitView>) -> AnyElement {
    let page = v_flex().flex_1().min_h_0();
    let status = match &folder.baseline {
        Baseline::Pending | Baseline::Taking => {
            return page
                .child(status_line(format!(
                    "Taking a snapshot of {}…",
                    shown_path(&folder.path)
                )))
                .into_any_element();
        }
        Baseline::TooBig(reason) => {
            let why = match reason {
                TooBig::Home => format!(
                    "{} is too broad to watch for changes. Open a project folder, or pick a repository below.",
                    shown_path(&folder.path)
                ),
                TooBig::Over { files, .. } => format!(
                    "{} has too many files to watch for changes (over {files}). Open a smaller folder, or pick a repository below.",
                    shown_path(&folder.path)
                ),
            };
            return page
                .id("folder-too-big")
                .overflow_y_scroll()
                .child(status_line(why))
                .child(repo_list(folder, false, cx))
                .into_any_element();
        }
        Baseline::Ready { since, files } => match folder.changes.len() {
            0 => format!(
                "No changes since {since} ({files} file{} watched).",
                plural(*files)
            ),
            n => format!("{n} file{} changed since {since}.", plural(n)),
        },
    };
    let rows: Vec<AnyElement> = folder
        .changes
        .iter()
        .map(|change| {
            let path = change.path.clone();
            let open = path.clone();
            let row = FileRow::new(
                format!("folder-file-{path}"),
                path.clone(),
                change.kind,
                folder.selected.as_ref() == Some(&path),
                move |app, event, _, cx| {
                    if event.click_count() >= 2 {
                        app.open_folder_file(&path, None, cx);
                    } else {
                        app.select_folder_file(path.clone(), cx);
                    }
                },
            );
            if change.kind == ChangeKind::Deleted {
                row.render(cx)
            } else {
                row.action(OPEN, move |app, _, _, cx| {
                    app.open_folder_file(&open, None, cx)
                })
                .render(cx)
            }
        })
        .collect();
    let diff = folder.diff.clone();
    let list = v_flex()
        .id("folder-changes")
        .px_1()
        .overflow_y_scroll()
        .children(rows)
        .when(folder.changes.is_empty(), |list| {
            list.child(repo_list(folder, false, cx))
        });
    page.child(status_line(status))
        .map(|page| match diff {
            Some(_) => page.child(list.flex_none().max_h(px(220.))),
            None => page.child(list.flex_1().min_h_0()),
        })
        .children(diff.map(|file| diff_view(app, file, cx)))
        .into_any_element()
}

/// The selected file against the baseline, in FerrumGIT's hunk view.
fn diff_view(
    app: &GitView,
    file: std::sync::Arc<ferrumgit_domain::FileDiff>,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let status = theme.status();
    let (word, word_color) = change_word(file.change, cx);
    let (path, line) = (file.path.clone(), first_line(&file));
    let deleted = file.change == ChangeKind::Deleted;
    v_flex()
        .flex_1()
        .min_h_0()
        .border_t_1()
        .border_color(colors.border)
        .child(
            h_flex()
                .flex_none()
                .h(px(36.))
                .gap_2()
                .px_3()
                .border_b_1()
                .border_color(colors.border)
                .text_ui_sm(cx)
                .child(div().text_color(word_color).child(word))
                .child(div().min_w_0().truncate().child(file.path.clone()))
                .child(
                    div()
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .text_color(status.created)
                        .child(format!("+{}", file.additions)),
                )
                .child(
                    div()
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .text_color(status.deleted)
                        .child(format!("−{}", file.deletions)),
                )
                .child(div().flex_1())
                .when(!deleted, |row| {
                    row.child(
                        ghost_button("folder-diff-open", "Open")
                            .tooltip(Tooltip::text("Open this file in the editor"))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.open_folder_file(&path, line, cx);
                            })),
                    )
                })
                .child(
                    div()
                        .id("folder-diff-close")
                        .p_1()
                        .rounded_sm()
                        .cursor_pointer()
                        .hover(|s| s.bg(colors.element_hover))
                        .tooltip(Tooltip::text("Close (Esc)"))
                        .child(Icon::Close.icon().size(IconSize::Small).color(Color::Muted))
                        .on_click(cx.listener(|app, _, _, cx| app.close_folder_diff(cx))),
                ),
        )
        .child(
            div()
                .id("folder-diff-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(code_diff(app, file, DiffSource::Commit, None, cx)),
        )
        .into_any_element()
}

/// The repositories below the folder. `page`: the Repositories page (says
/// so when there are none; elsewhere an empty list shows nothing).
fn repo_list(folder: &FolderMode, page: bool, cx: &mut Context<GitView>) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let Some(repos) = folder.repos.as_ref() else {
        return if page {
            status_line("Looking for repositories…")
        } else {
            div().into_any_element()
        };
    };
    if repos.is_empty() {
        return if page {
            status_line(format!(
                "{} is not a git repository, and none were found in its folders ({} levels down).",
                shown_path(&folder.path),
                super::scan::REPO_DEPTH
            ))
        } else {
            div().into_any_element()
        };
    }
    let rows = repos.iter().map(|repo| {
        let relative = repo
            .strip_prefix(&folder.path)
            .map_or_else(|_| repo.display().to_string(), |r| r.display().to_string());
        let target: PathBuf = repo.clone();
        h_flex()
            .id(ElementId::Name(format!("folder-repo-{relative}").into()))
            .gap_2()
            .px_3()
            .py_1p5()
            .cursor_pointer()
            .rounded_sm()
            .hover(|s| s.bg(colors.element_hover))
            .tooltip(Tooltip::text(format!("Show {}", shown_path(repo))))
            .child(
                Icon::Branch
                    .icon()
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(
                div().min_w_0().child(
                    Label::new(folder_name(repo))
                        .size(LabelSize::Small)
                        .truncate(),
                ),
            )
            .child(
                div().flex_1().min_w_0().child(
                    Label::new(relative)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate(),
                ),
            )
            .on_click(cx.listener(move |app, _, _, cx| app.pick_repo(target.clone(), cx)))
    });
    v_flex()
        .flex_none()
        .px_1()
        .pb_2()
        .child(
            div().px_2().pt_2().pb_1().child(
                Label::new(format!(
                    "REPOSITORIES IN {}",
                    folder_name(&folder.path).to_uppercase()
                ))
                .size(LabelSize::XSmall)
                .color(Color::Muted),
            ),
        )
        .children(rows)
        .into_any_element()
}

/// Above a repository picked from the folder: back to the folder.
pub(crate) fn picked_strip(app: &GitView, cx: &mut Context<GitView>) -> Option<AnyElement> {
    let folder = app.folder.as_ref()?;
    let picked = folder.picked.as_ref()?;
    app.state.active?;
    let colors = cx.theme().colors().clone();
    let relative = picked
        .strip_prefix(&folder.path)
        .map_or_else(|_| shown_path(picked), |r| r.display().to_string());
    Some(
        h_flex()
            .id("folder-back")
            .flex_none()
            .h(px(28.))
            .gap_1()
            .px_2()
            .border_b_1()
            .border_color(colors.border_variant)
            .bg(colors.title_bar_background)
            .cursor_pointer()
            .hover(|s| s.bg(colors.element_hover))
            .tooltip(Tooltip::text(
                "Back to the folder's changes and repositories",
            ))
            .child(
                Icon::ChevronLeft
                    .icon()
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(Label::new(folder_name(&folder.path)).size(LabelSize::Small))
            .child(
                Label::new(format!("/ {relative}"))
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .truncate(),
            )
            .on_click(cx.listener(|app, _, _, cx| app.back_to_folder(cx)))
            .into_any_element(),
    )
}
