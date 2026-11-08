pub mod dialogs;
pub mod fields;
pub mod request;
pub mod response;
pub mod shell;
pub mod sidebar;
pub mod widgets;

use crate::app::FerrumApi;
use ferrumapi_domain::HttpMethod;
use gpui::{App, Context, ElementId, Hsla, SharedString, Window, div, prelude::*, rgb};
use ui::{
    ActiveTheme as _, Checkbox, ContextMenu, DropdownMenu, DropdownStyle, Label, LabelCommon as _,
    LabelSize, ToggleState,
};

/// Postman-style method colour.
#[must_use]
pub fn method_color(m: HttpMethod, cx: &App) -> Hsla {
    match m {
        HttpMethod::Get => rgb(0x4ade80).into(),
        HttpMethod::Post => rgb(0xfbbf24).into(),
        HttpMethod::Put => rgb(0x60a5fa).into(),
        HttpMethod::Patch => rgb(0xc084fc).into(),
        HttpMethod::Delete => rgb(0xf87171).into(),
        HttpMethod::Head | HttpMethod::Options => cx.theme().colors().text_muted,
    }
}

/// Status-code colour (2xx success, 3xx info, 4xx warning, 5xx error).
#[must_use]
pub fn status_color(status: u16, cx: &App) -> Hsla {
    let s = cx.theme().status().clone();
    match status {
        200..=299 => s.success,
        300..=399 => s.info,
        400..=499 => s.warning,
        _ => s.error,
    }
}

/// A method label in its colour, e.g. in the tree and tab strip.
pub fn method_label(m: HttpMethod, cx: &App) -> impl IntoElement {
    div()
        .flex_none()
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(method_color(m, cx))
        .child(m.to_string())
}

/// Small muted help text.
pub fn hint(text: impl Into<SharedString>) -> Label {
    Label::new(text)
        .size(LabelSize::Small)
        .color(ui::Color::Muted)
}

/// Underlined tab (request sections, response views, dialog panes).
pub fn tab_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let colors = cx.theme().colors().clone();
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1p5()
        .h_full()
        .px_0p5()
        .border_b_2()
        .text_xs()
        .font_weight(gpui::FontWeight::MEDIUM)
        .cursor_pointer()
        .map(|this| {
            if selected {
                this.border_color(colors.text).text_color(colors.text)
            } else {
                this.border_color(gpui::transparent_black())
                    .text_color(colors.text_muted)
                    .hover(|s| s.text_color(colors.text))
            }
        })
        .child(label.into())
}

/// Small count pill next to a tab label.
pub fn count_pill(count: usize, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors().clone();
    div()
        .px_1()
        .rounded_sm()
        .bg(colors.element_background)
        .text_color(colors.text_muted)
        .text_xs()
        .child(count.to_string())
}

/// A dropdown that picks one of `options` (label, value).
pub fn select<T: Clone + 'static>(
    id: impl Into<ElementId>,
    current: impl Into<SharedString>,
    options: Vec<(SharedString, T)>,
    on_pick: impl Fn(&mut FerrumApi, T, &mut Context<FerrumApi>) + 'static,
    window: &mut Window,
    cx: &mut Context<FerrumApi>,
) -> DropdownMenu {
    let weak = cx.weak_entity();
    let on_pick = std::rc::Rc::new(on_pick);
    let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
        for (label, value) in options {
            let weak = weak.clone();
            let on_pick = on_pick.clone();
            menu = menu.entry(label, None, move |_, cx| {
                let value = value.clone();
                let on_pick = on_pick.clone();
                if let Err(error) = weak.update(cx, move |this, cx| {
                    on_pick(this, value, cx);
                    cx.notify();
                }) {
                    tracing::warn!(%error, "dropdown target dropped");
                }
            });
        }
        menu
    });
    DropdownMenu::new(id, current, menu).style(DropdownStyle::Outlined)
}

/// Builds a context menu whose entries act on the root view.
pub fn action_menu(
    window: &mut Window,
    cx: &mut App,
    weak: gpui::WeakEntity<FerrumApi>,
    entries: Vec<(SharedString, MenuHandler)>,
) -> gpui::Entity<ContextMenu> {
    ContextMenu::build(window, cx, move |mut menu, _, _| {
        for (label, handler) in entries {
            let weak = weak.clone();
            menu = menu.entry(label, None, move |window, cx| {
                let handler = handler.clone();
                if let Err(error) = weak.update(cx, move |this, cx| {
                    handler(this, window, cx);
                    cx.notify();
                }) {
                    tracing::warn!(%error, "menu target dropped");
                }
            });
        }
        menu
    })
}

pub type MenuHandler = std::rc::Rc<dyn Fn(&mut FerrumApi, &mut Window, &mut Context<FerrumApi>)>;

pub fn menu_item(
    label: impl Into<SharedString>,
    handler: impl Fn(&mut FerrumApi, &mut Window, &mut Context<FerrumApi>) + 'static,
) -> (SharedString, MenuHandler) {
    (label.into(), std::rc::Rc::new(handler))
}

/// A checkbox bound to the root view.
pub fn checkbox(
    id: impl Into<ElementId>,
    checked: bool,
    on_toggle: impl Fn(&mut FerrumApi, bool, &mut Context<FerrumApi>) + 'static,
    cx: &mut Context<FerrumApi>,
) -> Checkbox {
    Checkbox::new(
        id,
        if checked {
            ToggleState::Selected
        } else {
            ToggleState::Unselected
        },
    )
    .on_click(cx.listener(move |this, state: &ToggleState, _, cx| {
        on_toggle(this, *state == ToggleState::Selected, cx);
        cx.notify();
    }))
}
