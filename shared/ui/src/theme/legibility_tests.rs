//! Both built-in themes keep their text readable.
//!
//! - 4.5:1 (WCAG AA, normal text): body, muted and accent text on every
//!   surface; each status colour on its own tint and on the plain surfaces;
//!   regular text on every status tint; git-state colours; syntax colours on
//!   the editor; chip/button text on solid accent and status fills.
//! - 3:1 (WCAG AA for non-text UI, SC 1.4.11): placeholders, icons and editor
//!   line numbers, which are hints rather than content. Disabled text is
//!   exempt and not checked.

use gpui::Hsla;

use super::contrast::{MIN_TEXT_CONTRAST, MIN_UI_CONTRAST, contrast_over, readable_text_on};
use super::{Theme, dark, light};

fn check(theme: &Theme, failures: &mut Vec<String>, what: &str, fg: Hsla, bg: Hsla, min: f32) {
    let ratio = contrast_over(fg, bg, theme.colors().background);
    if ratio < min {
        failures.push(format!("{}: {what} is {ratio:.2}:1 (< {min})", theme.name));
    }
}

fn audit(theme: &Theme) -> Vec<String> {
    let c = theme.colors();
    let mut f = Vec::new();
    let surfaces = [
        ("background", c.background),
        ("surface", c.surface_background),
        ("elevated surface", c.elevated_surface_background),
        ("panel", c.panel_background),
        ("element", c.element_background),
        ("editor", c.editor_background),
        ("status bar", c.status_bar_background),
        ("title bar", c.title_bar_background),
        ("toolbar", c.toolbar_background),
        ("tab bar", c.tab_bar_background),
        ("active tab", c.tab_active_background),
        ("inactive tab", c.tab_inactive_background),
        ("hovered element", c.element_hover),
        ("pressed element", c.element_active),
        ("selected element", c.element_selected),
    ];
    for (name, bg) in surfaces {
        check(
            theme,
            &mut f,
            &format!("text on {name}"),
            c.text,
            bg,
            MIN_TEXT_CONTRAST,
        );
        for (label, fg) in [
            ("muted text", c.text_muted),
            ("accent text", c.text_accent),
            ("link", c.link_text_hover),
            ("added", c.version_control_added),
            ("modified", c.version_control_modified),
            ("deleted", c.version_control_deleted),
            ("renamed", c.version_control_renamed),
            ("conflict", c.version_control_conflict),
            ("ignored", c.version_control_ignored),
        ] {
            check(
                theme,
                &mut f,
                &format!("{label} on {name}"),
                fg,
                bg,
                MIN_TEXT_CONTRAST,
            );
        }
        for (label, fg) in [
            ("placeholder", c.text_placeholder),
            ("icon", c.icon),
            ("muted icon", c.icon_muted),
            ("accent icon", c.icon_accent),
        ] {
            check(
                theme,
                &mut f,
                &format!("{label} on {name}"),
                fg,
                bg,
                MIN_UI_CONTRAST,
            );
        }
    }

    // Solid accent fill (primary / selected segment buttons, HEAD chip).
    let accent_text = readable_text_on(c.text_accent);
    check(
        theme,
        &mut f,
        "text on accent fill",
        accent_text,
        c.text_accent,
        MIN_TEXT_CONTRAST,
    );

    for (name, fg, bg) in theme.status().pairs() {
        check(
            theme,
            &mut f,
            &format!("{name} on its tint"),
            fg,
            bg,
            MIN_TEXT_CONTRAST,
        );
        check(
            theme,
            &mut f,
            &format!("text on {name} tint"),
            c.text,
            bg,
            MIN_TEXT_CONTRAST,
        );
        for (surface, s) in &surfaces {
            check(
                theme,
                &mut f,
                &format!("{name} on {surface}"),
                fg,
                *s,
                MIN_TEXT_CONTRAST,
            );
        }
        let chip_text = readable_text_on(fg);
        check(
            theme,
            &mut f,
            &format!("text on {name} fill"),
            chip_text,
            fg,
            MIN_TEXT_CONTRAST,
        );
    }

    check(
        theme,
        &mut f,
        "line number on editor",
        c.editor_line_number,
        c.editor_background,
        MIN_UI_CONTRAST,
    );
    check(
        theme,
        &mut f,
        "active line number on active line",
        c.editor_active_line_number,
        c.editor_active_line_background,
        MIN_TEXT_CONTRAST,
    );
    for token in [
        "attribute",
        "boolean",
        "comment",
        "constant",
        "constructor",
        "enum",
        "function",
        "keyword",
        "number",
        "property",
        "string",
        "string.regex",
        "type",
        "diff.plus",
        "diff.minus",
    ] {
        let color = theme.syntax().color(token).expect("token has a colour");
        for (line, bg) in [
            ("editor", c.editor_background),
            ("active line", c.editor_active_line_background),
        ] {
            check(
                theme,
                &mut f,
                &format!("{token} on {line}"),
                color,
                bg,
                MIN_TEXT_CONTRAST,
            );
        }
    }
    f
}

#[test]
fn dark_theme_text_is_readable() {
    let failures = audit(&dark());
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn light_theme_text_is_readable() {
    let failures = audit(&light());
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn status_backgrounds_are_tints_not_the_foreground() {
    for theme in [dark(), light()] {
        for (name, fg, bg) in theme.status().pairs() {
            assert_ne!(fg, bg, "{}: {name}", theme.name);
            assert!(bg.is_opaque(), "{}: {name}", theme.name);
        }
    }
}

#[test]
fn theme_picks_dark_text_on_light_fills() {
    let theme = dark();
    let yellow = theme.status().warning;
    assert!(theme.readable_text_on(yellow).l < 0.5);
    let light = light();
    let deep = light.colors().text_accent;
    assert!(
        contrast_over(
            light.readable_text_on(deep),
            deep,
            light.colors().background
        ) >= 4.5
    );
}
