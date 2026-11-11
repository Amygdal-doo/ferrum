//! Volumes and networks: list, remove, prune unused.

use crate::app::FerrumContainers;
use crate::state::{Section, matches_any};
use crate::views::widgets::{
    button, cell, grow_cell, header_row, icon_button, muted, row, section_bar,
};
use ferrumdock_engine::format;
use gpui::{AnyElement, Context, SharedString, prelude::*, px};
use ui::{Color, IconName, Label, LabelCommon, LabelSize, h_flex, v_flex};

fn list(id: &'static str, loaded: bool, empty: &'static str, rows: Vec<AnyElement>) -> AnyElement {
    if !loaded {
        ferrum_ui::empty_state("Loading…", "").into_any_element()
    } else if rows.is_empty() {
        ferrum_ui::empty_state(empty, "").into_any_element()
    } else {
        v_flex()
            .id(id)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(rows)
            .into_any_element()
    }
}

fn trash(
    id: String,
    tooltip: &'static str,
    disabled: bool,
    cx: &mut Context<FerrumContainers>,
    on_click: impl Fn(&mut FerrumContainers, &mut Context<FerrumContainers>) + 'static,
) -> AnyElement {
    h_flex()
        .flex_none()
        .w(px(40.))
        .justify_end()
        .child(icon_button(
            SharedString::from(id),
            IconName::Trash,
            tooltip,
            disabled,
            cx,
            on_click,
        ))
        .into_any_element()
}

pub fn volumes(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    const COLUMNS: [(&str, Option<f32>); 6] = [
        ("NAME", None),
        ("PROJECT", Some(140.)),
        ("DRIVER", Some(80.)),
        ("SIZE", Some(90.)),
        ("IN USE", Some(90.)),
        ("", Some(40.)),
    ];
    let known: Vec<u64> = app.state.volumes.iter().filter_map(|v| v.size).collect();
    let total = (!known.is_empty()).then(|| format::bytes(known.iter().sum()));
    let pruning = app.state.busy.contains("prune:Volumes");
    let actions = vec![
        muted(total.map_or_else(String::new, |t| format!("{t} total"))).into_any_element(),
        button("prune-volumes", "Prune unused", pruning, cx, |app, cx| {
            app.confirm_prune(Section::Volumes, cx)
        })
        .into_any_element(),
    ];
    let rows = app
        .state
        .volumes
        .iter()
        .filter(|v| {
            matches_any(
                &app.state.filter,
                &[&v.name, v.project().unwrap_or_default(), &v.driver],
            )
        })
        .map(|v| {
            let in_use = v.ref_count.is_some_and(|n| n > 0);
            let usage = match v.ref_count {
                None => "–".to_owned(),
                Some(0) => "unused".to_owned(),
                Some(n) => format!("{n} container{}", if n == 1 { "" } else { "s" }),
            };
            let name = v.name.clone();
            row(SharedString::from(format!("volume-{}", v.name)), false, cx)
                .child(grow_cell(
                    Label::new(v.name.clone()).size(LabelSize::Small).truncate(),
                ))
                .child(cell(
                    140.,
                    v.project().unwrap_or("–").to_owned(),
                    Color::Muted,
                ))
                .child(cell(80., v.driver.clone(), Color::Muted))
                .child(cell(
                    90.,
                    v.size.map_or_else(|| "–".to_owned(), format::bytes),
                    Color::Default,
                ))
                .child(cell(
                    90.,
                    usage,
                    if in_use { Color::Default } else { Color::Muted },
                ))
                .child(trash(
                    format!("rm-volume-{}", v.name),
                    if in_use {
                        "In use by a container"
                    } else {
                        "Remove volume"
                    },
                    in_use || app.state.busy.contains(&v.name),
                    cx,
                    move |app, cx| app.confirm_remove_volume(&name, cx),
                ))
                .into_any_element()
        })
        .collect();
    v_flex()
        .size_full()
        .child(section_bar("Volumes", app.state.volumes.len(), actions, cx))
        .child(header_row(&COLUMNS, cx))
        .child(list(
            "volumes-list",
            app.state.loaded.contains(&Section::Volumes),
            "No volumes",
            rows,
        ))
        .into_any_element()
}

pub fn networks(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    const COLUMNS: [(&str, Option<f32>); 6] = [
        ("NAME", None),
        ("PROJECT", Some(140.)),
        ("DRIVER", Some(90.)),
        ("SCOPE", Some(70.)),
        ("ID", Some(110.)),
        ("", Some(40.)),
    ];
    let pruning = app.state.busy.contains("prune:Networks");
    let actions = vec![
        button("prune-networks", "Prune unused", pruning, cx, |app, cx| {
            app.confirm_prune(Section::Networks, cx)
        })
        .into_any_element(),
    ];
    let rows = app
        .state
        .networks
        .iter()
        .filter(|n| {
            matches_any(
                &app.state.filter,
                &[&n.name, n.project().unwrap_or_default(), &n.driver],
            )
        })
        .map(|n| {
            let builtin = n.is_builtin();
            let target = n.clone();
            let name = h_flex()
                .gap_2()
                .child(Label::new(n.name.clone()).size(LabelSize::Small).truncate())
                .when(builtin, |d| d.child(muted("built-in")))
                .when(n.internal, |d| d.child(muted("internal")));
            row(SharedString::from(format!("network-{}", n.id)), false, cx)
                .child(grow_cell(name))
                .child(cell(
                    140.,
                    n.project().unwrap_or("–").to_owned(),
                    Color::Muted,
                ))
                .child(cell(90., n.driver.clone(), Color::Muted))
                .child(cell(70., n.scope.clone(), Color::Muted))
                .child(cell(
                    110.,
                    ferrumdock_engine::format::short_id(&n.id).to_owned(),
                    Color::Muted,
                ))
                .child(trash(
                    format!("rm-network-{}", n.id),
                    if builtin {
                        "Built-in networks cannot be removed"
                    } else {
                        "Remove network"
                    },
                    builtin || app.state.busy.contains(&n.id),
                    cx,
                    move |app, cx| app.confirm_remove_network(&target, cx),
                ))
                .into_any_element()
        })
        .collect();
    v_flex()
        .size_full()
        .child(section_bar(
            "Networks",
            app.state.networks.len(),
            actions,
            cx,
        ))
        .child(header_row(&COLUMNS, cx))
        .child(list(
            "networks-list",
            app.state.loaded.contains(&Section::Networks),
            "No networks",
            rows,
        ))
        .into_any_element()
}
