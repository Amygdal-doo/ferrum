//! Colours and small pieces shared by the sidebar and the log view.

use ferrum_ui::gpui::{Div, Hsla};
use ferrum_ui::prelude::*;
use ferrumlogs_core::ansi::AnsiColor;
use ferrumlogs_core::{Health, Level};

/// The colour of source slot `slot` (chips in the list, dots in the sidebar).
pub fn source_color(slot: u32, cx: &App) -> Hsla {
    let c = cx.theme().colors();
    // No red: that means errors.
    let palette = [
        c.terminal_ansi_blue,
        c.terminal_ansi_magenta,
        c.terminal_ansi_cyan,
        c.terminal_ansi_green,
        c.terminal_ansi_yellow,
        c.terminal_ansi_bright_blue,
        c.terminal_ansi_bright_magenta,
        c.terminal_ansi_bright_cyan,
    ];
    palette[slot as usize % palette.len()]
}

pub fn level_color(level: Level, cx: &App) -> Hsla {
    let status = cx.theme().status();
    let colors = cx.theme().colors();
    match level {
        Level::Error => status.error,
        Level::Warn => status.warning,
        Level::Info => status.info,
        Level::Debug => colors.text_muted,
        Level::Trace => colors.text_placeholder,
    }
}

pub fn health_color(health: Health, cx: &App) -> Hsla {
    let status = cx.theme().status();
    match health {
        Health::Running => status.success,
        Health::Failed => status.error,
        Health::Stopped | Health::Idle => cx.theme().colors().text_placeholder,
    }
}

/// A small round status dot.
pub fn dot(color: Hsla) -> Div {
    div().flex_none().size(px(7.)).rounded_full().bg(color)
}

/// A coloured source tag, fixed width so the text column lines up.
pub fn source_chip(name: &str, slot: u32, width: f32, cx: &App) -> Div {
    let color = source_color(slot, cx);
    div().flex_none().w(px(width)).overflow_hidden().child(
        div()
            .max_w_full()
            .px_1()
            .rounded_sm()
            .bg(color.opacity(0.14))
            .text_color(color)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(name.to_owned()),
    )
}

/// A count pill, e.g. errors of a source.
pub fn count_pill(count: u64, color: Hsla) -> Div {
    let text = if count > 9_999 {
        format!("{}k", count / 1000)
    } else {
        count.to_string()
    };
    div()
        .flex_none()
        .px_1()
        .rounded_sm()
        .bg(color.opacity(0.15))
        .text_color(color)
        .text_xs()
        .child(text)
}

pub fn ansi_color(color: AnsiColor, cx: &App) -> Hsla {
    let c = cx.theme().colors();
    match color.0 {
        0 => c.terminal_ansi_black,
        1 => c.terminal_ansi_red,
        2 => c.terminal_ansi_green,
        3 => c.terminal_ansi_yellow,
        4 => c.terminal_ansi_blue,
        5 => c.terminal_ansi_magenta,
        6 => c.terminal_ansi_cyan,
        7 => c.terminal_ansi_white,
        8 => c.terminal_ansi_bright_black,
        9 => c.terminal_ansi_bright_red,
        10 => c.terminal_ansi_bright_green,
        11 => c.terminal_ansi_bright_yellow,
        12 => c.terminal_ansi_bright_blue,
        13 => c.terminal_ansi_bright_magenta,
        14 => c.terminal_ansi_bright_cyan,
        _ => c.terminal_ansi_bright_white,
    }
}
