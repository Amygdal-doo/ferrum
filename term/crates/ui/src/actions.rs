//! FerrumTerm's shortcuts as GPUI actions, bound in the [`KEY_CONTEXT`] key
//! context: while a terminal has focus they win over the host app's global
//! bindings (⌘W closes the pane, not the host's tab), and elsewhere in the
//! host they do nothing.

use crate::input::{Action, Shortcut};
use crate::view::TermView;
use ferrum_ui::gpui::{Context, Div, InteractiveElement as _, KeyBinding, Window, actions};

/// Key context of the terminal view.
pub const KEY_CONTEXT: &str = "FerrumTerm";

/// Keystrokes and the shortcut each one triggers. `input::translate` maps
/// the same keystrokes (a fallback when [`crate::init`] was not called).
pub(crate) const BINDINGS: &[(&str, Shortcut)] = &[
    ("cmd-t", Shortcut::NewTab),
    ("cmd-n", Shortcut::NewSession),
    ("cmd-d", Shortcut::SplitRight),
    ("cmd-shift-d", Shortcut::SplitDown),
    ("cmd-w", Shortcut::CloseBlock),
    ("cmd-shift-w", Shortcut::CloseBlock),
    ("cmd-]", Shortcut::NextBlock),
    ("cmd-[", Shortcut::PrevBlock),
    ("cmd-shift-]", Shortcut::NextTab),
    ("cmd-}", Shortcut::NextTab),
    ("cmd-shift-[", Shortcut::PrevTab),
    ("cmd-{", Shortcut::PrevTab),
    ("cmd-1", Shortcut::Tab(0)),
    ("cmd-2", Shortcut::Tab(1)),
    ("cmd-3", Shortcut::Tab(2)),
    ("cmd-4", Shortcut::Tab(3)),
    ("cmd-5", Shortcut::Tab(4)),
    ("cmd-6", Shortcut::Tab(5)),
    ("cmd-7", Shortcut::Tab(6)),
    ("cmd-8", Shortcut::Tab(7)),
    ("cmd-9", Shortcut::Tab(8)),
    ("cmd-v", Shortcut::Paste),
    ("cmd-shift-v", Shortcut::Paste),
    ("cmd-c", Shortcut::Copy),
    ("cmd-shift-c", Shortcut::CopyScreen),
    ("cmd-=", Shortcut::FontBigger),
    ("cmd-+", Shortcut::FontBigger),
    ("cmd--", Shortcut::FontSmaller),
    ("cmd-0", Shortcut::FontReset),
    ("cmd-shift-s", Shortcut::NextSession),
    ("cmd-b", Shortcut::ToggleSidebar),
    ("cmd-pageup", Shortcut::ScrollPageUp),
    ("cmd-pagedown", Shortcut::ScrollPageDown),
    ("cmd-f", Shortcut::Find),
    ("cmd-g", Shortcut::FindNext),
    ("cmd-shift-g", Shortcut::FindPrevious),
    ("cmd-up", Shortcut::PreviousCommand),
    ("cmd-down", Shortcut::NextCommand),
];

/// Declares the actions, maps each to its [`Shortcut`], and registers a
/// handler for every one on the view's root element.
macro_rules! shortcut_actions {
    ($($action:ident => $shortcut:expr),* $(,)?) => {
        actions!(ferrum_term, [$($action),*]);

        fn binding(keys: &str, shortcut: Shortcut) -> KeyBinding {
            $(if shortcut == $shortcut {
                return KeyBinding::new(keys, $action, Some(KEY_CONTEXT));
            })*
            unreachable!("every shortcut has an action")
        }

        /// Routes every action to the view (only while the terminal itself
        /// has focus: a sidebar or dialog text field keeps its own keys).
        pub(crate) fn on_actions(root: Div, cx: &mut Context<TermView>) -> Div {
            root$(.on_action(cx.listener(|view: &mut TermView, _: &$action, window: &mut Window, cx| {
                view.shortcut_action($shortcut, window, cx);
            })))*
            // The menu bar's Edit › Copy / Paste.
            .on_action(cx.listener(|view: &mut TermView, _: &ferrum_ui::menus::EditCopy, window: &mut Window, cx| {
                view.shortcut_action(Shortcut::Copy, window, cx);
            }))
            .on_action(cx.listener(|view: &mut TermView, _: &ferrum_ui::menus::EditPaste, window: &mut Window, cx| {
                view.shortcut_action(Shortcut::Paste, window, cx);
            }))
        }
    };
}

shortcut_actions! {
    NewTab => Shortcut::NewTab,
    NewSession => Shortcut::NewSession,
    SplitRight => Shortcut::SplitRight,
    SplitDown => Shortcut::SplitDown,
    ClosePane => Shortcut::CloseBlock,
    NextPane => Shortcut::NextBlock,
    PreviousPane => Shortcut::PrevBlock,
    NextTab => Shortcut::NextTab,
    PreviousTab => Shortcut::PrevTab,
    ActivateTab1 => Shortcut::Tab(0),
    ActivateTab2 => Shortcut::Tab(1),
    ActivateTab3 => Shortcut::Tab(2),
    ActivateTab4 => Shortcut::Tab(3),
    ActivateTab5 => Shortcut::Tab(4),
    ActivateTab6 => Shortcut::Tab(5),
    ActivateTab7 => Shortcut::Tab(6),
    ActivateTab8 => Shortcut::Tab(7),
    ActivateTab9 => Shortcut::Tab(8),
    Paste => Shortcut::Paste,
    Copy => Shortcut::Copy,
    CopyScreen => Shortcut::CopyScreen,
    IncreaseFontSize => Shortcut::FontBigger,
    DecreaseFontSize => Shortcut::FontSmaller,
    ResetFontSize => Shortcut::FontReset,
    NextSession => Shortcut::NextSession,
    ScrollPageUp => Shortcut::ScrollPageUp,
    ScrollPageDown => Shortcut::ScrollPageDown,
    ToggleSidebar => Shortcut::ToggleSidebar,
    Find => Shortcut::Find,
    FindNext => Shortcut::FindNext,
    FindPrevious => Shortcut::FindPrevious,
    PreviousCommand => Shortcut::PreviousCommand,
    NextCommand => Shortcut::NextCommand,
}

/// FerrumTerm's key bindings, all in the [`KEY_CONTEXT`] context.
/// [`crate::init`] registers them; hosts with their own keymap can register
/// these (or a subset) themselves.
#[must_use]
pub fn key_bindings() -> Vec<KeyBinding> {
    BINDINGS
        .iter()
        .map(|(keys, shortcut)| binding(keys, *shortcut))
        .collect()
}

impl TermView {
    fn shortcut_action(&mut self, shortcut: Shortcut, window: &mut Window, cx: &mut Context<Self>) {
        // The find shortcuts also work in the find bar's field.
        let in_find = self
            .find
            .as_ref()
            .is_some_and(|bar| bar.focused(window, cx));
        if in_find || self.focus_handle.is_focused(window) {
            match shortcut {
                Shortcut::Find => return self.open_find(window, cx),
                Shortcut::FindNext => return self.find_step(crate::find::Step::Older, cx),
                Shortcut::FindPrevious => return self.find_step(crate::find::Step::Newer, cx),
                _ => {}
            }
        }
        if !self.focus_handle.is_focused(window) {
            cx.propagate();
            return;
        }
        match shortcut {
            Shortcut::PreviousCommand => {
                return self.jump_command(crate::blocks::Jump::Previous, cx);
            }
            Shortcut::NextCommand => return self.jump_command(crate::blocks::Jump::Next, cx),
            _ => {}
        }
        self.ime_marked = None;
        self.act(Action::Shortcut(shortcut), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_ui::gpui::Keystroke;

    #[test]
    fn bindings_match_the_fallback_key_handler() {
        for (keys, shortcut) in BINDINGS {
            let keystroke = Keystroke::parse(keys).expect("valid keystroke");
            assert_eq!(
                crate::input::translate(&keystroke, true),
                Some(Action::Shortcut(*shortcut)),
                "{keys}"
            );
        }
        assert_eq!(key_bindings().len(), BINDINGS.len());
    }
}
