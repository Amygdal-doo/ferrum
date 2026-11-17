//! Images: list with size and tags, pull, remove, prune dangling.

use crate::app::FerrumContainers;
use crate::commands::resources::image_label;
use crate::state::{Section, matches_any};
use crate::views::chrome::card_header;
use crate::views::widgets::{button, cell, grow_cell, header_row, icon_button, muted, row};
use ferrum_ui::components::{Color, IconName, Label, LabelCommon, LabelSize, h_flex, v_flex};
use ferrumdock_engine::{Image, format};
use gpui::{AnyElement, Context, SharedString, Window, prelude::*};

const COLUMNS: [(&str, Option<f32>); 6] = [
    ("REPOSITORY:TAG", None),
    ("ID", Some(110.)),
    ("SIZE", Some(90.)),
    ("CREATED", Some(120.)),
    ("USED BY", Some(90.)),
    ("", Some(116.)),
];

/// The images card: its header and its body.
pub fn render(
    app: &FerrumContainers,
    window: &Window,
    cx: &mut Context<FerrumContainers>,
) -> (AnyElement, AnyElement) {
    let now = chrono::Utc::now().timestamp();
    let mut images: Vec<&Image> = app
        .state
        .images
        .iter()
        .filter(|i| {
            let tags = i.tags.join(" ");
            matches_any(&app.state.filter, &[&tags, i.short_id()])
        })
        .collect();
    images.sort_by(|a, b| {
        a.is_dangling()
            .cmp(&b.is_dangling())
            .then_with(|| a.tags.first().cmp(&b.tags.first()))
    });
    let total: u64 = app.state.images.iter().map(|i| i.size).sum();
    let dangling = app.state.images.iter().filter(|i| i.is_dangling()).count();
    let pruning = app.state.busy.contains("prune:Images");
    let actions = vec![
        muted(format!(
            "{} total · {dangling} dangling",
            format::bytes(total)
        ))
        .into_any_element(),
        button("registries", "Registries…", false, cx, |app, cx| {
            app.open_registries(cx)
        })
        .into_any_element(),
        button("pull-image", "Pull…", false, cx, |app, cx| {
            app.open_pull_dialog(cx)
        })
        .into_any_element(),
        button("prune-images", "Prune dangling", pruning, cx, |app, cx| {
            app.confirm_prune(Section::Images, cx)
        })
        .into_any_element(),
    ];
    let body: AnyElement = if !app.state.loaded.contains(&Section::Images) {
        ferrum_ui::empty_state("Loading images…", "").into_any_element()
    } else if images.is_empty() {
        ferrum_ui::empty_state("No images", "Pull one with Pull….").into_any_element()
    } else {
        v_flex()
            .id("images-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(
                images
                    .into_iter()
                    .map(|image| image_row(app, image, now, cx)),
            )
            .into_any_element()
    };
    let count = Some(app.state.images.len());
    let header = card_header(app, "Images", count, actions, window, cx);
    let list = v_flex()
        .size_full()
        .child(header_row(&COLUMNS, cx))
        .child(body)
        .into_any_element();
    (header, list)
}

fn image_row(
    app: &FerrumContainers,
    image: &Image,
    now: i64,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    let busy = app.state.busy.contains(&image.id);
    let extra_tags = image.tags.len().saturating_sub(1);
    let label = h_flex()
        .gap_2()
        .child(
            Label::new(image_label(image))
                .size(LabelSize::Small)
                .color(if image.is_dangling() {
                    Color::Muted
                } else {
                    Color::Default
                })
                .truncate(),
        )
        .when(extra_tags > 0, |d| {
            d.child(muted(format!(
                "+{extra_tags} tag{}",
                if extra_tags == 1 { "" } else { "s" }
            )))
        });
    let used = match image.containers {
        n if n < 0 => "–".to_owned(),
        0 => "unused".to_owned(),
        n => format!("{n} container{}", if n == 1 { "" } else { "s" }),
    };
    let target = image.clone();
    let (tag, history) = (image.clone(), image.clone());
    let run_ref = image
        .tags
        .first()
        .cloned()
        .unwrap_or_else(|| image.id.clone());
    row(SharedString::from(format!("image-{}", image.id)), false, cx)
        .child(grow_cell(label))
        .child(cell(110., image.short_id().to_owned(), Color::Muted))
        .child(cell(90., format::bytes(image.size), Color::Default))
        .child(cell(120., format::age(image.created, now), Color::Muted))
        .child(cell(90., used, Color::Muted))
        .child(
            h_flex()
                .flex_none()
                .w(gpui::px(116.))
                .gap_0p5()
                .justify_end()
                .child(icon_button(
                    SharedString::from(format!("run-image-{}", image.id)),
                    IconName::PlayFilled,
                    "Run a container…",
                    false,
                    cx,
                    move |app, cx| app.open_run_dialog(Some(run_ref.clone()), cx),
                ))
                .child(icon_button(
                    SharedString::from(format!("tag-image-{}", image.id)),
                    IconName::Pencil,
                    "Tag…",
                    busy,
                    cx,
                    move |app, cx| app.open_tag_dialog(&tag, cx),
                ))
                .child(icon_button(
                    SharedString::from(format!("history-image-{}", image.id)),
                    IconName::HistoryRerun,
                    "History (layers)",
                    false,
                    cx,
                    move |app, cx| app.open_image_history(&history, cx),
                ))
                .child(icon_button(
                    SharedString::from(format!("rm-image-{}", image.id)),
                    IconName::Trash,
                    "Remove image",
                    busy,
                    cx,
                    move |app, cx| app.confirm_remove_image(&target, cx),
                )),
        )
        .into_any_element()
}
