//! Hover tooltips: plain text, or a title with the key binding of an action.

use gpui::{
    Action, AnyView, App, AppContext as _, Context, IntoElement, Keystroke, ParentElement, Render,
    SharedString, Styled, Window, div,
};

use super::traits::{StyledExt, StyledTypography};
use crate::theme::{ActiveTheme, theme_settings};

/// A tooltip view: a title, optional detail line and optional shortcut.
pub struct Tooltip {
    title: SharedString,
    meta: Option<SharedString>,
    action: Option<Box<dyn Action>>,
}

impl Tooltip {
    /// A builder for `.tooltip(...)` that shows `text`.
    pub fn text(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView {
        let text = text.into();
        move |_, cx| Self::simple(text.clone(), cx)
    }

    /// A text tooltip view.
    pub fn simple(text: impl Into<SharedString>, cx: &mut App) -> AnyView {
        Self::view(text.into(), None, None, cx)
    }

    /// `title` with the shortcut bound to `action`, if any.
    pub fn for_action(
        title: impl Into<SharedString>,
        action: &dyn Action,
        cx: &mut App,
    ) -> AnyView {
        Self::view(title.into(), None, Some(action.boxed_clone()), cx)
    }

    /// `title` with a muted second line.
    pub fn with_meta(
        title: impl Into<SharedString>,
        action: Option<&dyn Action>,
        meta: impl Into<SharedString>,
        cx: &mut App,
    ) -> AnyView {
        let action = action.map(|a| a.boxed_clone());
        Self::view(title.into(), Some(meta.into()), action, cx)
    }

    fn view(
        title: SharedString,
        meta: Option<SharedString>,
        action: Option<Box<dyn Action>>,
        cx: &mut App,
    ) -> AnyView {
        cx.new(|_| Self {
            title,
            meta,
            action,
        })
        .into()
    }
}

/// `cmd-shift-p` → `⌘⇧P` on macOS; `Ctrl+Shift+P` elsewhere.
pub fn format_keystroke(keystroke: &Keystroke) -> String {
    let m = &keystroke.modifiers;
    let key = match keystroke.key.as_str() {
        "enter" => "↵".to_owned(),
        "escape" => "Esc".to_owned(),
        "backspace" => "⌫".to_owned(),
        "tab" => "⇥".to_owned(),
        "up" => "↑".to_owned(),
        "down" => "↓".to_owned(),
        "left" => "←".to_owned(),
        "right" => "→".to_owned(),
        "space" => "Space".to_owned(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    };
    if cfg!(target_os = "macos") {
        let mut out = String::new();
        for (on, glyph) in [
            (m.control, "⌃"),
            (m.alt, "⌥"),
            (m.shift, "⇧"),
            (m.platform, "⌘"),
        ] {
            if on {
                out.push_str(glyph);
            }
        }
        out + &key
    } else {
        let mut parts: Vec<&str> = Vec::new();
        for (on, name) in [
            (m.control, "Ctrl"),
            (m.alt, "Alt"),
            (m.shift, "Shift"),
            (m.platform, "Super"),
        ] {
            if on {
                parts.push(name);
            }
        }
        parts.push(&key);
        parts.join("+")
    }
}

impl Render for Tooltip {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shortcut = self.action.as_ref().and_then(|action| {
            let binding = window.highest_precedence_binding_for_action(action.as_ref())?;
            let keys: Vec<String> = binding
                .keystrokes()
                .iter()
                .map(|k| format_keystroke(k.inner()))
                .collect();
            Some(keys.join(" "))
        });
        let colors = cx.theme().colors();
        let font = theme_settings(cx).ui_font.clone();
        div().pl_2().pt_2p5().child(
            div()
                .v_flex()
                .elevation_2(cx)
                .font(font)
                .text_ui(cx)
                .text_color(colors.text)
                .py_1()
                .px_2()
                .child(
                    div().h_flex().gap_4().child(self.title.clone()).children(
                        shortcut.map(|keys| div().text_color(colors.text_muted).child(keys)),
                    ),
                )
                .children(self.meta.clone().map(|meta| {
                    div()
                        .text_ui_sm(cx)
                        .text_color(colors.text_muted)
                        .child(meta)
                })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_shortcuts() {
        let keystroke = Keystroke::parse("cmd-shift-p").expect("parse");
        let text = format_keystroke(&keystroke);
        if cfg!(target_os = "macos") {
            assert_eq!(text, "⇧⌘P");
        } else {
            assert_eq!(text, "Shift+Super+P");
        }
        let enter = Keystroke::parse("enter").expect("parse");
        assert_eq!(format_keystroke(&enter), "↵");
    }
}
