//! [`ContextMenu`]: a keyboard-navigable list of commands shown by
//! [`super::PopoverMenu`], [`super::right_click_menu`] and
//! [`super::DropdownMenu`].

use std::rc::Rc;

use gpui::{
    Action, AnyElement, App, AppContext as _, Context, DismissEvent, Entity, EventEmitter,
    FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div, prelude::FluentBuilder, px,
};

use super::traits::{StyledExt, StyledTypography};
use super::{Color, Icon, IconName, IconPosition, IconSize, Label, LabelCommon, LabelSize};
use crate::menu::{Cancel, Confirm, SelectFirst, SelectLast, SelectNext, SelectPrevious};
use crate::theme::{ActiveTheme, theme_settings};

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

struct Entry {
    label: SharedString,
    icon: Option<IconName>,
    /// An agent's or model maker's logo, after the icon.
    logo: Option<crate::AgentLogo>,
    /// A small badge at the end, e.g. a plan name.
    badge: Option<SharedString>,
    toggle: Option<(bool, IconPosition)>,
    action: Option<Box<dyn Action>>,
    disabled: bool,
    handler: Handler,
}

enum Item {
    Entry(Entry),
    Header(SharedString),
    Label(SharedString),
    Separator,
}

impl Item {
    fn selectable(&self) -> bool {
        matches!(self, Item::Entry(entry) if !entry.disabled)
    }
}

pub struct ContextMenu {
    items: Vec<Item>,
    selected: Option<usize>,
    focus_handle: FocusHandle,
    _blur: Option<Subscription>,
}

impl EventEmitter<DismissEvent> for ContextMenu {}

impl Focusable for ContextMenu {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ContextMenu {
    /// Creates a menu; `build` adds the items. The menu dismisses itself when
    /// it loses focus, on Escape and after an entry runs.
    pub fn build(
        window: &mut Window,
        cx: &mut App,
        build: impl FnOnce(Self, &mut Window, &mut Context<Self>) -> Self,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let focus_handle = cx.focus_handle();
            let blur = cx.on_blur(&focus_handle, window, |_, _, cx| cx.emit(DismissEvent));
            let menu = Self {
                items: Vec::new(),
                selected: None,
                focus_handle,
                _blur: Some(blur),
            };
            build(menu, window, cx)
        })
    }

    fn push_entry(mut self, entry: Entry) -> Self {
        self.items.push(Item::Entry(entry));
        self
    }

    /// A command; `action` only supplies the shortcut shown on the right.
    pub fn entry(
        self,
        label: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.push_entry(Entry {
            label: label.into(),
            icon: None,
            logo: None,
            badge: None,
            toggle: None,
            action,
            disabled: false,
            handler: Rc::new(handler),
        })
    }

    /// A command with a leading icon.
    pub fn icon_entry(
        self,
        label: impl Into<SharedString>,
        icon: IconName,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.push_entry(Entry {
            label: label.into(),
            icon: Some(icon),
            logo: None,
            badge: None,
            toggle: None,
            action: None,
            disabled: false,
            handler: Rc::new(handler),
        })
    }

    /// A command with a check mark when `toggled`.
    pub fn toggleable_entry(
        self,
        label: impl Into<SharedString>,
        toggled: bool,
        position: IconPosition,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.push_entry(Entry {
            label: label.into(),
            icon: None,
            logo: None,
            badge: None,
            toggle: Some((toggled, position)),
            action,
            disabled: false,
            handler: Rc::new(handler),
        })
    }

    /// Leads the entry added last with `logo` (an agent or model maker's).
    pub fn with_logo(mut self, logo: Option<crate::AgentLogo>) -> Self {
        if let Some(Item::Entry(entry)) = self.items.last_mut() {
            entry.logo = logo;
        }
        self
    }

    /// Ends the entry added last with a small badge (e.g. a plan, "Free").
    pub fn with_badge(mut self, badge: impl Into<SharedString>) -> Self {
        if let Some(Item::Entry(entry)) = self.items.last_mut() {
            entry.badge = Some(badge.into());
        }
        self
    }

    /// A greyed-out entry that cannot be chosen.
    pub fn disabled_entry(self, label: impl Into<SharedString>) -> Self {
        self.push_entry(Entry {
            label: label.into(),
            icon: None,
            logo: None,
            badge: None,
            toggle: None,
            action: None,
            disabled: true,
            handler: Rc::new(|_, _| {}),
        })
    }

    /// A small section heading.
    pub fn header(mut self, title: impl Into<SharedString>) -> Self {
        self.items.push(Item::Header(title.into()));
        self
    }

    /// Plain, unselectable text.
    pub fn label(mut self, text: impl Into<SharedString>) -> Self {
        self.items.push(Item::Label(text.into()));
        self
    }

    pub fn separator(mut self) -> Self {
        self.items.push(Item::Separator);
        self
    }

    /// Moves the keyboard selection to the next selectable item.
    pub(crate) fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let count = self.items.len();
        if count == 0 {
            return;
        }
        let mut ix = match (self.selected, forward) {
            (Some(ix), true) => ix + 1,
            (Some(ix), false) => ix + count - 1,
            (None, true) => 0,
            (None, false) => count - 1,
        } % count;
        for _ in 0..count {
            if self.items[ix].selectable() {
                self.selected = Some(ix);
                cx.notify();
                return;
            }
            ix = if forward { ix + 1 } else { ix + count - 1 };
            ix %= count;
        }
    }

    pub(crate) fn select_edge(&mut self, last: bool, cx: &mut Context<Self>) {
        self.selected = None;
        self.step(!last, cx);
    }

    /// The keyboard-selected item, if any.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    fn confirm_at(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Item::Entry(entry)) = self.items.get(ix) else {
            return;
        };
        if entry.disabled {
            return;
        }
        let handler = entry.handler.clone();
        cx.emit(DismissEvent);
        handler(window, cx);
    }

    fn render_entry(
        &self,
        ix: usize,
        entry: &Entry,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        let highlighted = self.selected == Some(ix);
        let check = |position: IconPosition| {
            entry
                .toggle
                .filter(|(_, at)| *at == position)
                .map(|(on, _)| {
                    div().flex_none().size(px(14.)).children(on.then(|| {
                        Icon::new(IconName::Check)
                            .size(IconSize::Small)
                            .color(Color::Accent)
                    }))
                })
        };
        let shortcut = entry.action.as_ref().and_then(|action| {
            let binding = window.highest_precedence_binding_for_action(action.as_ref())?;
            let keys: Vec<String> = binding
                .keystrokes()
                .iter()
                .map(|k| super::tooltip::format_keystroke(k.inner()))
                .collect();
            Some(keys.join(" "))
        });
        let label_color = if entry.disabled {
            Color::Disabled
        } else {
            Color::Default
        };
        div()
            .id(("ferrum-menu-entry", ix))
            .h_flex()
            .w_full()
            .gap_1p5()
            .px(px(6.))
            .rounded_sm()
            .when(highlighted, |el| el.bg(colors.ghost_element_selected))
            .when(!entry.disabled, |el| {
                el.cursor_pointer()
                    .hover(|s| s.bg(colors.ghost_element_hover))
                    .on_click(
                        cx.listener(move |menu, _, window, cx| menu.confirm_at(ix, window, cx)),
                    )
            })
            .children(check(IconPosition::Start))
            .children(
                entry
                    .icon
                    .map(|icon| Icon::new(icon).size(IconSize::Small).color(Color::Muted)),
            )
            .children(entry.logo.map(|logo| crate::agent_logo(logo, px(14.), cx)))
            .child(
                div()
                    .flex_1()
                    .child(Label::new(entry.label.clone()).color(label_color)),
            )
            .children(
                entry
                    .badge
                    .clone()
                    .map(|badge| crate::badge(badge, colors.text_muted, cx)),
            )
            .children(
                shortcut.map(|keys| Label::new(keys).size(LabelSize::Small).color(Color::Muted)),
            )
            .children(check(IconPosition::End))
            .into_any_element()
    }
}

impl Render for ContextMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let font = theme_settings(cx).ui_font.family.clone();
        let rows: Vec<AnyElement> = self
            .items
            .iter()
            .enumerate()
            .map(|(ix, item)| match item {
                Item::Entry(entry) => self.render_entry(ix, entry, window, cx),
                Item::Header(title) => div()
                    .px(px(6.))
                    .pt_1()
                    .child(
                        Label::new(title.clone())
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .into_any_element(),
                Item::Label(text) => div()
                    .px(px(6.))
                    .child(Label::new(text.clone()).color(Color::Muted))
                    .into_any_element(),
                Item::Separator => div()
                    .my_1()
                    .h(px(1.))
                    .w_full()
                    .bg(colors.border_variant)
                    .into_any_element(),
            })
            .collect();

        div()
            .id("ferrum-context-menu")
            .key_context("menu")
            .track_focus(&self.focus_handle)
            .occlude()
            .v_flex()
            .min_w(px(160.))
            .py_1()
            .px_1()
            .elevation_2(cx)
            .font_family(font)
            .text_ui(cx)
            .on_action(cx.listener(|menu, _: &SelectNext, _, cx| menu.step(true, cx)))
            .on_action(cx.listener(|menu, _: &SelectPrevious, _, cx| menu.step(false, cx)))
            .on_action(cx.listener(|menu, _: &SelectFirst, _, cx| menu.select_edge(false, cx)))
            .on_action(cx.listener(|menu, _: &SelectLast, _, cx| menu.select_edge(true, cx)))
            .on_action(cx.listener(|menu, _: &Confirm, window, cx| {
                if let Some(ix) = menu.selected {
                    menu.confirm_at(ix, window, cx);
                }
            }))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(DismissEvent)))
            .on_mouse_down_out(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
            .children(rows)
    }
}
