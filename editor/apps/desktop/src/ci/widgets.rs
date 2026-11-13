//! Small shared pieces: status icons and a value dropdown.

use crate::ci::CiView;
use ferrum_ui::components::{DropdownStyle, Icon, IconSize};
use ferrum_ui::prelude::*;
use ferrumci_core::{CheckState, RunState};
use std::rc::Rc;

pub fn state_color(state: RunState) -> Color {
    match state {
        RunState::Success => Color::Success,
        RunState::Failure => Color::Error,
        RunState::InProgress | RunState::ActionRequired => Color::Warning,
        RunState::Queued | RunState::Cancelled | RunState::Skipped => Color::Muted,
    }
}

pub fn state_icon(state: RunState) -> Icon {
    let name = match state {
        RunState::Queued => IconName::Clock,
        RunState::InProgress => IconName::ArrowCircle,
        RunState::Success => IconName::Check,
        RunState::Failure => IconName::XCircle,
        RunState::Cancelled => IconName::Stop,
        RunState::Skipped => IconName::Dash,
        RunState::ActionRequired => IconName::Warning,
    };
    Icon::new(name)
        .size(IconSize::Small)
        .color(state_color(state))
}

pub fn check_icon(state: CheckState) -> Icon {
    let (name, color) = match state {
        CheckState::None => (IconName::Circle, Color::Muted),
        CheckState::Neutral => (IconName::Dash, Color::Muted),
        CheckState::Success => (IconName::Check, Color::Success),
        CheckState::Pending => (IconName::ArrowCircle, Color::Warning),
        CheckState::Failure => (IconName::XCircle, Color::Error),
    };
    Icon::new(name).size(IconSize::Small).color(color)
}

/// A dropdown whose entries set a value on the app.
pub fn select<T: Clone + 'static>(
    id: impl Into<ElementId>,
    current: impl Into<SharedString>,
    options: Vec<(SharedString, T)>,
    on_pick: impl Fn(&mut CiView, T, &mut Context<CiView>) + 'static,
    window: &mut Window,
    cx: &mut Context<CiView>,
) -> DropdownMenu {
    let weak = cx.weak_entity();
    let on_pick = Rc::new(on_pick);
    let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
        for (label, value) in options {
            let weak = weak.clone();
            let on_pick = on_pick.clone();
            menu = menu.entry(label, None, move |_, cx| {
                let value = value.clone();
                let on_pick = on_pick.clone();
                weak.update(cx, move |this, cx| {
                    on_pick(this, value, cx);
                    cx.notify();
                })
                .ok();
            });
        }
        menu
    });
    DropdownMenu::new(id, current, menu).style(DropdownStyle::Outlined)
}
