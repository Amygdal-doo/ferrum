//! Key tree under a key-value (Redis) connection: SCAN pattern box, loaded
//! count, `:`-grouped folders, type badges and TTLs, "Load more".

use crate::app::FerrumData;
use ferrum_ui::prelude::*;
use ferrumdata_application::key_tree::{KeyTreeRow, key_tree_rows};
use ferrumdata_domain::keyvalue::format_duration;
use ferrumdata_domain::{KeyEntry, KeyTtl, ProfileId};
use std::borrow::Cow;

/// Max tree rows rendered; beyond this the user is asked to narrow down.
const MAX_RENDERED: usize = 500;
const INDENT: f32 = 12.;

impl FerrumData {
    pub fn render_key_tree(
        &mut self,
        profile_id: ProfileId,
        filter: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let error_color = cx.theme().status().error;
        let input = self.key_pattern_input(profile_id, window, cx);
        let muted = |text: SharedString| {
            div()
                .py_1()
                .pl(px(28.))
                .text_xs()
                .text_color(colors.text_muted)
                .child(text)
        };
        let Some(browser) = self.state.key_browsers.get(&profile_id) else {
            return muted("Scanning keys…".into()).into_any_element();
        };

        let needle = filter.to_lowercase();
        let keys: Cow<'_, [KeyEntry]> = if needle.is_empty() {
            Cow::Borrowed(&browser.keys)
        } else {
            Cow::Owned(
                browser
                    .keys
                    .iter()
                    .filter(|k| k.name.to_lowercase().contains(&needle))
                    .cloned()
                    .collect(),
            )
        };
        let (rows, cut) = key_tree_rows(&keys, &browser.expanded, !needle.is_empty(), MAX_RENDERED);
        let active_key = self
            .state
            .active_tab()
            .filter(|t| t.profile_id == Some(profile_id))
            .and_then(|t| t.key.clone());

        let count = match (browser.total, browser.complete) {
            (_, true) => format!("Keys: {} matching", browser.keys.len()),
            (Some(total), false) => {
                format!("Keys: {} loaded · {total} in DB", browser.keys.len())
            }
            (None, false) => format!("Keys: {} loaded", browser.keys.len()),
        };
        let status = if browser.loading {
            Some(muted("Scanning…".into()).into_any_element())
        } else if let Some(e) = &browser.error {
            Some(
                h_flex()
                    .gap_2()
                    .py_1()
                    .pl(px(28.))
                    .text_xs()
                    .text_color(error_color)
                    .child(div().truncate().child(e.clone()))
                    .child(
                        Button::new(
                            SharedString::from(format!("keys-retry-{profile_id}")),
                            "Retry",
                        )
                        .label_size(LabelSize::XSmall)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.scan_keys(profile_id, true, cx);
                        })),
                    )
                    .into_any_element(),
            )
        } else if !browser.complete {
            Some(
                div()
                    .pl(px(24.))
                    .py_0p5()
                    .child(
                        Button::new(
                            SharedString::from(format!("keys-more-{profile_id}")),
                            "Load more",
                        )
                        .label_size(LabelSize::XSmall)
                        .style(ButtonStyle::Outlined)
                        .tooltip(Tooltip::text("Continue the SCAN from where it stopped"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.scan_keys(profile_id, false, cx);
                        })),
                    )
                    .into_any_element(),
            )
        } else {
            None
        };
        let empty = browser.keys.is_empty() && !browser.loading && browser.error.is_none();

        let mut items: Vec<AnyElement> = Vec::with_capacity(rows.len());
        for (i, row) in rows.into_iter().enumerate() {
            let element = match row {
                KeyTreeRow::Folder {
                    prefix,
                    label,
                    depth,
                    keys: n,
                    expanded,
                } => h_flex()
                    .id(SharedString::from(format!("kfolder-{profile_id}-{i}")))
                    .gap_1()
                    .rounded_sm()
                    .py_px()
                    .pl(px(10. + INDENT * depth as f32))
                    .pr_2()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .hover(|s| s.bg(colors.element_hover).text_color(colors.text))
                    .tooltip(Tooltip::text(format!("{prefix}*  ({n} loaded keys)")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_key_folder(profile_id, prefix.clone(), cx);
                    }))
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(IconSize::XSmall),
                    )
                    .child(div().truncate().child(if label.is_empty() {
                        "(empty)".to_owned()
                    } else {
                        label
                    }))
                    .child(div().flex_none().child(format!("({n})")))
                    .into_any_element(),
                KeyTreeRow::Key {
                    index,
                    label,
                    depth,
                } => {
                    let entry = &keys[index];
                    let active = active_key.as_deref() == Some(entry.name.as_str());
                    let ttl = match entry.ttl {
                        KeyTtl::ExpiresIn(d) => Some(format_duration(d)),
                        KeyTtl::Persistent | KeyTtl::Missing => None,
                    };
                    let tip = if entry.binary {
                        format!("{} · not UTF-8: cannot be opened here", entry.name)
                    } else {
                        format!(
                            "{} · {} · {}",
                            entry.name,
                            entry.kind.label(),
                            entry.ttl.label()
                        )
                    };
                    let name = entry.name.clone();
                    let binary = entry.binary;
                    h_flex()
                        .id(SharedString::from(format!("key-{profile_id}-{i}")))
                        .gap_1p5()
                        .rounded_sm()
                        .py_px()
                        .pl(px(28. + INDENT * depth as f32))
                        .pr_2()
                        .text_xs()
                        .when(active, |this| this.bg(colors.element_selected))
                        .when(!active, |this| {
                            this.text_color(colors.text_muted)
                                .hover(|s| s.bg(colors.element_hover).text_color(colors.text))
                        })
                        .tooltip(Tooltip::text(tip))
                        .when(!binary, |this| {
                            this.on_click(cx.listener(move |this, _, _, cx| {
                                this.open_key(profile_id, name.clone(), cx);
                            }))
                        })
                        .child(
                            div()
                                .flex_none()
                                .w(px(38.))
                                .text_size(px(10.))
                                .text_color(colors.text_muted)
                                .child(entry.kind.badge().to_owned()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(if label.is_empty() {
                                    "(empty)".to_owned()
                                } else {
                                    label
                                }),
                        )
                        .children(ttl.map(|t| {
                            div()
                                .flex_none()
                                .text_size(px(10.))
                                .text_color(colors.text_muted)
                                .child(t)
                        }))
                        .into_any_element()
                }
            };
            items.push(element);
        }

        v_flex()
            .pb_1()
            .child(
                h_flex()
                    .gap_1()
                    .py_0p5()
                    .pl(px(24.))
                    .pr_1()
                    .child(div().flex_1().min_w_0().child(input))
                    .child(
                        IconButton::new(
                            SharedString::from(format!("keys-scan-{profile_id}")),
                            IconName::MagnifyingGlass,
                        )
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(
                            "Scan keys matching the pattern (SCAN … MATCH)",
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.scan_keys(profile_id, true, cx);
                        })),
                    )
                    .child(
                        IconButton::new(
                            SharedString::from(format!("keys-reload-{profile_id}")),
                            IconName::RotateCw,
                        )
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Rescan from the start"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.scan_keys(profile_id, true, cx);
                        })),
                    ),
            )
            .child(
                div()
                    .py_0p5()
                    .pl(px(24.))
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(count),
            )
            .when(empty, |this| this.child(muted("No keys match.".into())))
            .children(items)
            .when(cut, |this| {
                this.child(muted(
                    "More keys loaded than shown — narrow the pattern or the filter".into(),
                ))
            })
            .children(status)
            .into_any_element()
    }
}
