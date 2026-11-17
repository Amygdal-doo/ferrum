//! Files tab of a container: a folder listing (folders first), the path
//! field, Up / Refresh / Upload…, and a viewer for the file clicked last
//! with Download.

use crate::app::FerrumContainers;
use crate::views::widgets::{button, cell, grow_cell, header_row, icon_button, muted, row};
use ferrum_ui::components::{
    Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumdock_engine::archive::EntryKind;
use ferrumdock_engine::files::{FileEntry, VIEW_LIMIT};
use ferrumdock_engine::format;
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

const COLUMNS: [(&str, Option<f32>); 4] = [
    ("NAME", None),
    ("SIZE", Some(80.)),
    ("MODE", Some(96.)),
    ("MODIFIED", Some(110.)),
];

fn entry_row(
    i: usize,
    entry: &FileEntry,
    selected: bool,
    now: i64,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    let icon = match entry.kind {
        EntryKind::Dir => IconName::Folder,
        EntryKind::Symlink => IconName::Link,
        EntryKind::File | EntryKind::Other => IconName::File,
    };
    let name = match &entry.link {
        Some(target) => format!("{} → {target}", entry.name),
        None => entry.name.clone(),
    };
    let size = if entry.kind == EntryKind::File {
        format::bytes(entry.size)
    } else {
        String::new()
    };
    let modified = if entry.mtime > 0 {
        format::age(entry.mtime, now)
    } else {
        String::new()
    };
    let open = entry.name.clone();
    row(SharedString::from(format!("file-{i}")), selected, cx)
        .cursor_pointer()
        .child(grow_cell(
            h_flex()
                .gap_2()
                .child(Icon::new(icon).size(IconSize::Small).color(
                    if entry.kind == EntryKind::Dir {
                        Color::Accent
                    } else {
                        Color::Muted
                    },
                ))
                .child(Label::new(name).size(LabelSize::Small).truncate()),
        ))
        .child(cell(80., size, Color::Muted))
        .child(cell(96., entry.mode_string(), Color::Muted))
        .child(cell(110., modified, Color::Muted))
        .on_click(cx.listener(move |app, _, _, cx| app.open_file_entry(&open, cx)))
        .into_any_element()
}

fn preview(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> Option<AnyElement> {
    let files = &app.state.detail.as_ref()?.files;
    let (path, content) = files.preview.as_ref()?;
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let (summary, body): (String, AnyElement) = match content {
        None => (
            "Reading…".into(),
            muted("Reading the file…").into_any_element(),
        ),
        Some(Err(error)) => (
            String::new(),
            Label::new(error.clone())
                .size(LabelSize::Small)
                .color(Color::Error)
                .into_any_element(),
        ),
        Some(Ok(file)) => {
            let mut summary = format::bytes(file.size);
            if file.truncated {
                summary.push_str(&format!(" · first {} shown", format::bytes(VIEW_LIMIT)));
            }
            let body = if file.text().is_some() {
                div()
                    .id("file-view")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .child(app.inputs.file_view.clone())
                    .into_any_element()
            } else {
                ferrum_ui::empty_state("Binary file", "Download it to open it on this Mac.")
                    .into_any_element()
            };
            (summary, body)
        }
    };
    let download = path.clone();
    let header = h_flex()
        .flex_none()
        .px_3()
        .py_1()
        .gap_2()
        .border_b_1()
        .border_color(colors.border_variant)
        .child(
            Icon::new(IconName::File)
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(Label::new(path.clone()).size(LabelSize::Small).truncate()),
        )
        .child(muted(summary))
        .child(button(
            "file-download",
            "Download…",
            files.transfer.is_some(),
            cx,
            move |app, cx| app.download_file(download.clone(), cx),
        ))
        .child(icon_button(
            "file-close",
            IconName::Close,
            "Close the viewer",
            false,
            cx,
            |app, cx| app.close_preview(cx),
        ));
    Some(
        v_flex()
            .flex_none()
            .h(px(280.))
            .border_t_1()
            .border_color(colors.border_variant)
            .child(header)
            .child(body)
            .into_any_element(),
    )
}

pub fn render(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let Some(detail) = app.state.detail.as_ref() else {
        return div().into_any_element();
    };
    let files = &detail.files;
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let busy = files.transfer.is_some();
    let toolbar = h_flex()
        .flex_none()
        .px_3()
        .py_1()
        .gap_2()
        .child(icon_button(
            "files-up",
            IconName::ArrowUp,
            "Parent folder",
            files.path == "/",
            cx,
            |app, cx| app.files_up(cx),
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .px_2()
                .py_0p5()
                .rounded_md()
                .bg(ferrum_ui::hover_fill(cx))
                .child(app.inputs.files_path.clone()),
        )
        .child(icon_button(
            "files-refresh",
            IconName::RotateCw,
            "Refresh",
            files.loading,
            cx,
            |app, cx| {
                let path = app
                    .state
                    .detail
                    .as_ref()
                    .map_or_else(|| "/".to_owned(), |d| d.files.path.clone());
                app.load_dir(path, cx);
            },
        ))
        .child(button("files-upload", "Upload…", busy, cx, |app, cx| {
            app.choose_upload(cx)
        }));
    let now = chrono::Utc::now().timestamp();
    let selected = files
        .preview
        .as_ref()
        .and_then(|(p, _)| p.rsplit('/').next())
        .map(str::to_owned);
    let body: AnyElement = match &files.listing {
        None => ferrum_ui::empty_state("Reading the folder…", "").into_any_element(),
        Some(Err(error)) => {
            ferrum_ui::empty_state("Cannot list this folder", error.clone()).into_any_element()
        }
        Some(Ok(listing)) if listing.entries.is_empty() => {
            ferrum_ui::empty_state("Empty folder", listing.path.clone()).into_any_element()
        }
        Some(Ok(listing)) => {
            let rows: Vec<AnyElement> = listing
                .entries
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    let selected = selected.as_deref() == Some(entry.name.as_str());
                    entry_row(i, entry, selected, now, cx)
                })
                .collect();
            v_flex()
                .id("files-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(rows)
                .into_any_element()
        }
    };
    let status = match (&files.transfer, &files.listing) {
        (Some(what), _) => Some(what.clone()),
        (None, Some(Ok(listing))) if listing.partial => Some(format!(
            "Only the first {} entries are shown (the folder is very large).",
            listing.entries.len()
        )),
        _ => None,
    };
    v_flex()
        .flex_1()
        .min_h_0()
        .child(toolbar)
        .child(header_row(&COLUMNS, cx))
        .child(body)
        .children(status.map(|s| {
            div()
                .flex_none()
                .px_3()
                .py_0p5()
                .border_t_1()
                .border_color(colors.border_variant)
                .child(muted(s))
        }))
        .children(preview(app, cx))
        .into_any_element()
}
