//! The merged log view: filter toolbar, the virtualised line list
//! (`uniform_list`), the selected line's details and the status line.

use crate::logs::LogsView;
use crate::logs::widgets::{ansi_color, level_color, source_chip};
use ferrum_ui::gpui::{
    AnyElement, FontStyle, FontWeight, HighlightStyle, ListSizingBehavior, ScrollEvent, StyledText,
};
use ferrum_ui::prelude::*;
use ferrumlogs_core::filter::segments;
use ferrumlogs_core::{Level, LogLine, Session, Stream};
use std::ops::Range;

/// Bytes of a line drawn in the list (the details pane shows all of it).
const MAX_SHOWN: usize = 2_000;
const CHIP_WIDTH: f32 = 104.;
const TIME_WIDTH: f32 = 92.;

pub fn render(app: &mut LogsView, _window: &mut Window, cx: &mut Context<LogsView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    v_flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(toolbar(app, cx))
        .child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .bg(colors.editor_background)
                .font_buffer(cx)
                .text_buffer(cx)
                .child(list(app, cx))
                .when(!app.follow, |d| d.child(jump_button(app.unseen, cx))),
        )
        .children(details(app, cx))
        .into_any_element()
}

fn toolbar(app: &LogsView, cx: &mut Context<LogsView>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let filter = app
        .session()
        .map(|s| s.filter().clone())
        .unwrap_or_default();
    let error = app.session().and_then(|s| s.filter_error.clone());
    let errors_only = filter.levels == ferrumlogs_core::Levels::ERRORS_ONLY;
    let mut levels = h_flex().gap_0p5();
    for level in [Level::Error, Level::Warn, Level::Info, Level::Debug] {
        levels = levels.child(
            Button::new(
                SharedString::from(format!("level-{}", level.name())),
                level.name(),
            )
            .style(ButtonStyle::Subtle)
            .size(ButtonSize::Compact)
            .label_size(LabelSize::Small)
            .toggle_state(filter.levels.get(level))
            .color(if filter.levels.get(level) {
                level_label_color(level)
            } else {
                Color::Placeholder
            })
            .tooltip(Tooltip::text(match level {
                Level::Info => "Info and lines without a level",
                Level::Debug => "Debug and trace",
                _ => "",
            }))
            .on_click(cx.listener(move |app, _, _, cx| {
                app.update_filter(cx, |f| f.levels.toggle(level));
            })),
        );
    }
    h_flex()
        .flex_none()
        .px_2()
        .py_1()
        .gap_2()
        .border_b_1()
        .border_color(colors.border)
        .bg(colors.toolbar_background)
        .child(
            h_flex()
                .flex_1()
                .min_w(px(160.))
                .pl_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if error.is_some() {
                    cx.theme().status().error_border
                } else {
                    colors.border
                })
                .bg(colors.editor_background)
                .child(
                    Icon::new(IconName::MagnifyingGlass)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .child(div().flex_1().min_w_0().child(app.search.clone()))
                .when_some(error, |d, error| {
                    d.child(
                        Label::new(error)
                            .size(LabelSize::XSmall)
                            .color(Color::Error)
                            .truncate(),
                    )
                })
                .child(
                    IconButton::new("toggle-case", IconName::CaseSensitive)
                        .icon_size(IconSize::Small)
                        .toggle_state(filter.case_sensitive)
                        .tooltip(Tooltip::text("Match case"))
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.update_filter(cx, |f| f.case_sensitive = !f.case_sensitive);
                        })),
                )
                .child(
                    IconButton::new("toggle-regex", IconName::Regex)
                        .icon_size(IconSize::Small)
                        .toggle_state(filter.regex)
                        .tooltip(Tooltip::text("Regular expression"))
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.update_filter(cx, |f| f.regex = !f.regex);
                        })),
                ),
        )
        .child(levels)
        .child(
            Button::new("errors-only", "Errors only")
                .style(ButtonStyle::Outlined)
                .size(ButtonSize::Compact)
                .label_size(LabelSize::Small)
                .toggle_state(errors_only)
                .tooltip(Tooltip::text("Only errors (⌘E)"))
                .on_click(cx.listener(|app, _, _, cx| app.toggle_errors_only(cx))),
        )
        .child(
            h_flex()
                .gap_0p5()
                .child(
                    IconButton::new("toggle-follow", IconName::ArrowDown)
                        .icon_size(IconSize::Small)
                        .toggle_state(app.follow)
                        .tooltip(Tooltip::text("Follow new lines (End)"))
                        .on_click(cx.listener(|app, _, _, cx| {
                            let follow = !app.follow;
                            app.set_follow(follow, cx);
                        })),
                )
                .child(
                    IconButton::new("copy-selection", IconName::Copy)
                        .icon_size(IconSize::Small)
                        .disabled(app.selection.is_none())
                        .tooltip(Tooltip::text("Copy selected lines (⌘C)"))
                        .on_click(cx.listener(|app, _, _, cx| app.copy_selection(cx))),
                )
                .child(
                    IconButton::new("export", IconName::Download)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Export visible lines (⌘S)"))
                        .on_click(cx.listener(|app, _, _, cx| app.export(cx))),
                )
                .child(
                    IconButton::new("clear", IconName::Eraser)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Clear (⌘K)"))
                        .on_click(cx.listener(|app, _, _, cx| app.clear(cx))),
                ),
        )
}

fn level_label_color(level: Level) -> Color {
    match level {
        Level::Error => Color::Error,
        Level::Warn => Color::Warning,
        Level::Info => Color::Info,
        Level::Debug | Level::Trace => Color::Muted,
    }
}

fn list(app: &LogsView, cx: &mut Context<LogsView>) -> AnyElement {
    let Some(session) = app.session() else {
        return empty_state(
            "No project open",
            "Pick one with the project switcher (⌥⌘O)",
        )
        .into_any_element();
    };
    let count = session.view.len();
    if count == 0 {
        let (title, detail) = if session.sources.is_empty() {
            ("No sources", "Add one, or pick a suggestion in the sidebar")
        } else if !session.buffer.is_empty() {
            ("No lines match", "Change the search or the level filters")
        } else if session.running_count() == 0 {
            ("Nothing running", "Start a source to follow its output")
        } else {
            ("Waiting for output…", "")
        };
        return empty_state(title, detail).into_any_element();
    }
    div()
        .id("log-list-wrapper")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .on_scroll(cx.listener(|app, event: &ScrollEvent, window, cx| {
            let delta = event.delta.pixel_delta(window.line_height());
            app.on_wheel(delta.y > px(0.), delta.y < px(0.), cx);
        }))
        .child(
            uniform_list(
                "log-lines",
                count,
                cx.processor(|app, range: Range<usize>, _, cx| {
                    let Some(session) = app.session() else {
                        return Vec::new();
                    };
                    let selection = app.selection.map(|s| s.range());
                    range
                        .filter_map(|row| {
                            let line = session.row(row)?;
                            let selected =
                                selection.as_ref().is_some_and(|r| r.contains(&line.seq));
                            Some(log_row(session, line, row, selected, cx))
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&app.scroll)
            .with_sizing_behavior(ListSizingBehavior::Infer)
            .flex_1(),
        )
        .into_any_element()
}

fn log_row(
    session: &Session,
    line: &LogLine,
    row: usize,
    selected: bool,
    cx: &mut Context<LogsView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let name = session
        .by_slot(line.source)
        .map_or("?", |s| s.config.name.as_str());
    let marker = if line.stream == Stream::Stderr {
        theme.status().error.opacity(0.7)
    } else {
        gpui_transparent()
    };
    h_flex()
        .id(("line", row))
        .w_full()
        .px_2()
        .gap_2()
        .whitespace_nowrap()
        .overflow_hidden()
        .border_l_2()
        .border_color(marker)
        .cursor_pointer()
        .when(selected, |d| d.bg(colors.element_selected))
        .when(!selected, |d| d.hover(|s| s.bg(colors.element_hover)))
        .on_click(cx.listener(move |app, event: &ClickEvent, _, cx| {
            app.select_row(row, event.modifiers().shift, cx);
        }))
        .child(source_chip(name, line.source, CHIP_WIDTH, cx))
        .child(
            div()
                .flex_none()
                .w(px(TIME_WIDTH))
                .text_color(colors.text_muted)
                .children(line.timestamp.clone()),
        )
        .child(
            div()
                .flex_none()
                .w(px(28.))
                .when_some(line.level, |d, level| {
                    d.text_color(level_color(level, cx))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(level.short())
                }),
        )
        .when_some(line.tag.clone(), |d, tag| {
            d.child(div().flex_none().text_color(colors.text_muted).child(tag))
        })
        .child(line_text(session, line, cx))
        .when(!line.fields.is_empty(), |d| {
            let mut inline = line.fields_inline();
            truncate_at(&mut inline, MAX_SHOWN);
            d.child(
                div()
                    .flex_none()
                    .text_color(colors.text_placeholder)
                    .child(inline),
            )
        })
        .into_any_element()
}

fn gpui_transparent() -> ferrum_ui::gpui::Hsla {
    ferrum_ui::gpui::transparent_black()
}

fn truncate_at(text: &mut String, max: usize) {
    if text.len() > max {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
}

/// The text with ANSI colours and search matches combined.
fn line_text(session: &Session, line: &LogLine, cx: &App) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let mut text = line.text.clone();
    truncate_at(&mut text, MAX_SHOWN);
    let shown = text.len();
    let matches = session.matcher().highlights(&text);
    let styles: Vec<_> = line
        .spans
        .iter()
        .filter(|(r, _)| r.start < shown && r.end <= shown)
        .cloned()
        .collect();
    let mut highlights = Vec::new();
    for segment in segments(shown, &styles, &matches) {
        let mut highlight = HighlightStyle::default();
        if let Some(style) = segment.style {
            highlight.color = style.fg.map(|c| ansi_color(c, cx));
            highlight.font_weight = style.bold.then_some(FontWeight::BOLD);
            highlight.fade_out = style.dim.then_some(0.4);
            highlight.font_style = style.italic.then_some(FontStyle::Italic);
        }
        if segment.matched {
            highlight.background_color = Some(colors.search_match_background);
        }
        highlights.push((segment.range, highlight));
    }
    let system = line.stream == Stream::System;
    div()
        .flex_none()
        .when(system, |d| d.text_color(colors.text_muted).italic())
        .child(StyledText::new(text).with_highlights(highlights))
        .into_any_element()
}

fn jump_button(unseen: usize, cx: &mut Context<LogsView>) -> impl IntoElement {
    let label = if unseen > 0 {
        format!("Jump to latest ({unseen} new)")
    } else {
        "Jump to latest".to_owned()
    };
    div()
        .absolute()
        .bottom(px(12.))
        .right(px(20.))
        .debug_selector(|| "jump-to-latest".into())
        .child(
            Button::new("jump-to-latest", label)
                .style(ButtonStyle::Filled)
                .size(ButtonSize::Medium)
                .label_size(LabelSize::Small)
                .start_icon(Icon::new(IconName::ArrowDown).size(IconSize::Small))
                .on_click(cx.listener(|app, _, _, cx| app.set_follow(true, cx))),
        )
}

/// The selected line in full, with its JSON fields.
fn details(app: &LogsView, cx: &mut Context<LogsView>) -> Option<AnyElement> {
    let session = app.session()?;
    let selection = app.selection?;
    if selection.anchor != selection.head {
        return None;
    }
    let line = session.buffer.get(selection.anchor)?;
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let name = session
        .by_slot(line.source)
        .map_or("?", |s| s.config.name.as_str());
    let copy = session.format_line(line);
    let original = line.original.clone();
    let mut fields = v_flex().gap_0p5();
    for (key, value) in &line.fields {
        fields = fields.child(
            h_flex()
                .gap_2()
                .items_start()
                .child(
                    div()
                        .flex_none()
                        .w(px(140.))
                        .overflow_hidden()
                        .text_color(colors.text_muted)
                        .child(key.clone()),
                )
                .child(div().flex_1().min_w_0().child(value.clone())),
        );
    }
    Some(
        v_flex()
            .flex_none()
            .h(px(190.))
            .border_t_1()
            .border_color(colors.border)
            .bg(colors.panel_background)
            .child(
                h_flex()
                    .px_3()
                    .py_1()
                    .gap_2()
                    .border_b_1()
                    .border_color(colors.border_variant)
                    .font_buffer(cx)
                    .text_buffer(cx)
                    .child(source_chip(name, line.source, CHIP_WIDTH, cx))
                    .children(line.timestamp.clone())
                    .when_some(line.level, |d, level| {
                        d.child(div().text_color(level_color(level, cx)).child(level.name()))
                    })
                    .children(line.tag.clone())
                    .child(div().flex_1())
                    .child(
                        Button::new("copy-line", "Copy line")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .label_size(LabelSize::Small)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.copy_text(copy.clone(), cx);
                            })),
                    )
                    .when_some(original, |d, original| {
                        d.child(
                            Button::new("copy-json", "Copy JSON")
                                .style(ButtonStyle::Subtle)
                                .size(ButtonSize::Compact)
                                .label_size(LabelSize::Small)
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.copy_text(original.clone(), cx);
                                })),
                        )
                    })
                    .child(
                        IconButton::new("close-details", IconName::Close)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.selection = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                v_flex()
                    .id("details-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .font_buffer(cx)
                    .text_buffer(cx)
                    .child(div().w_full().child(line.text.clone()))
                    .when(!line.fields.is_empty(), |d| d.child(fields)),
            )
            .into_any_element(),
    )
}

pub fn status_line(app: &LogsView, cx: &mut Context<LogsView>) -> impl IntoElement {
    let left = match app.session() {
        Some(session) => {
            let mut text = format!("{} of {} lines", session.view.len(), session.buffer.len());
            if session.buffer.dropped > 0 {
                text.push_str(&format!(" · {} oldest dropped", session.buffer.dropped));
            }
            if !app.follow {
                text.push_str(" · paused");
            }
            text
        }
        None => "No project".to_owned(),
    };
    let right = app.session().map_or_else(String::new, |session| {
        let running = session.running_count();
        format!("{running} of {} sources running", session.sources.len())
    });
    status_bar(cx)
        .child(Label::new(left).size(LabelSize::Small).color(Color::Muted))
        .child(Label::new(right).size(LabelSize::Small).color(Color::Muted))
}
