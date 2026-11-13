//! Menus that float over the window: [`PopoverMenu`] (opened by clicking a
//! button) and [`right_click_menu`] (opened at the pointer).

use std::rc::Rc;

use gpui::{
    Anchor, AnyElement, AnyView, App, ClickEvent, DismissEvent, ElementId, Entity, Focusable,
    InteractiveElement, IntoElement, ManagedView, MouseButton, ParentElement, Pixels, Point,
    RenderOnce, Styled, Subscription, Window, anchored, deferred, div, prelude::FluentBuilder, px,
};

use super::{Clickable, Toggleable};

/// A button that can open a popover.
pub trait PopoverTrigger: IntoElement + Clickable + Toggleable + 'static {}

impl<T: IntoElement + Clickable + Toggleable + 'static> PopoverTrigger for T {}

/// The open menu of one popover, kept between frames.
#[derive(Default)]
struct PopoverState {
    open: Option<(AnyView, Subscription)>,
    /// Where a right-click menu opens.
    position: Point<Pixels>,
    /// The press that closed the menu also lands on the trigger; ignore it.
    ignore_next_click: bool,
}

impl PopoverState {
    fn close(&mut self) {
        self.open = None;
    }
}

/// Shows `menu`, focuses it, and closes it (restoring focus) on dismiss.
fn show<M: ManagedView>(
    state: &Entity<PopoverState>,
    menu: Entity<M>,
    window: &mut Window,
    cx: &mut App,
) {
    let previous = window.focused(cx);
    let weak_state = state.downgrade();
    let subscription = window.subscribe(&menu, cx, move |menu, _: &DismissEvent, window, cx| {
        let had_focus = menu.focus_handle(cx).contains_focused(window, cx);
        if let Some(state) = weak_state.upgrade() {
            state.update(cx, |state, cx| {
                state.close();
                cx.notify();
            });
        }
        if had_focus && let Some(previous) = &previous {
            window.focus(previous, cx);
        }
    });
    window.focus(&menu.focus_handle(cx), cx);
    state.update(cx, |state, cx| {
        state.open = Some((menu.into(), subscription));
        cx.notify();
    });
}

/// The floating layer: `view` anchored at the origin of its parent.
fn layer(view: AnyView, anchor: Anchor) -> impl IntoElement {
    deferred(
        anchored()
            .anchor(anchor)
            .snap_to_window_with_margin(px(8.))
            .child(div().occlude().py(px(2.)).child(view)),
    )
    .with_priority(1)
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type RightClickBuilder<M> = Rc<dyn Fn(&mut Window, &mut App) -> Entity<M>>;
type MenuBuilder<M> = Rc<dyn Fn(&mut Window, &mut App) -> Option<Entity<M>>>;
type TriggerBuilder =
    Box<dyn FnOnce(bool, Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>) -> AnyElement>;

/// A trigger button that toggles a menu below (or above) it.
#[derive(IntoElement)]
pub struct PopoverMenu<M: ManagedView> {
    id: ElementId,
    trigger: Option<TriggerBuilder>,
    menu: Option<MenuBuilder<M>>,
    anchor: Anchor,
    full_width: bool,
}

impl<M: ManagedView> PopoverMenu<M> {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            trigger: None,
            menu: None,
            anchor: Anchor::TopLeft,
            full_width: false,
        }
    }

    pub fn trigger<T: PopoverTrigger>(mut self, trigger: T) -> Self {
        self.trigger = Some(Box::new(move |open, on_click| {
            trigger
                .toggle_state(open)
                .on_click(move |event, window, cx| on_click(event, window, cx))
                .into_any_element()
        }));
        self
    }

    /// Builds the menu each time it opens.
    pub fn menu(
        mut self,
        build: impl Fn(&mut Window, &mut App) -> Option<Entity<M>> + 'static,
    ) -> Self {
        self.menu = Some(Rc::new(build));
        self
    }

    /// Which corner of the menu meets the trigger (default: top-left, so the
    /// menu opens below the trigger's left edge).
    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Stretch the trigger wrapper to the container width.
    pub fn full_width(mut self, full_width: bool) -> Self {
        self.full_width = full_width;
        self
    }
}

impl<M: ManagedView> RenderOnce for PopoverMenu<M> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| PopoverState::default());
        let open = state.read(cx).open.as_ref().map(|(view, _)| view.clone());
        let builder = self.menu;
        let on_click: ClickHandler = {
            let state = state.clone();
            Rc::new(move |_, window, cx| {
                let (is_open, ignore) = {
                    let s = state.read(cx);
                    (s.open.is_some(), s.ignore_next_click)
                };
                state.update(cx, |s, _| s.ignore_next_click = false);
                if is_open {
                    state.update(cx, |s, cx| {
                        s.close();
                        cx.notify();
                    });
                } else if !ignore
                    && let Some(menu) = builder.as_ref().and_then(|build| build(window, cx))
                {
                    show(&state, menu, window, cx);
                }
            })
        };
        let trigger = self.trigger.map(|build| build(open.is_some(), on_click));
        let anchor = self.anchor;
        let press_state = state.clone();

        div()
            .relative()
            .flex()
            .flex_none()
            .when(self.full_width, |el| el.w_full())
            .capture_any_mouse_down(move |_, _, cx| {
                press_state.update(cx, |s, _| s.ignore_next_click = s.open.is_some());
            })
            .children(trigger)
            .when_some(open, |el, view| {
                let corner = div().absolute().size_0();
                let corner = match anchor {
                    Anchor::TopRight => corner.right_0().top_full(),
                    Anchor::BottomLeft => corner.left_0().top_0(),
                    Anchor::BottomRight => corner.right_0().top_0(),
                    _ => corner.left_0().top_full(),
                };
                el.child(corner.child(layer(view, anchor)))
            })
    }
}

type RightClickTrigger = Box<dyn FnOnce(bool, &mut Window, &mut App) -> AnyElement>;

/// Wraps an element so right-clicking it opens a menu at the pointer.
#[derive(IntoElement)]
pub struct RightClickMenu<M: ManagedView> {
    id: ElementId,
    trigger: Option<RightClickTrigger>,
    menu: Option<RightClickBuilder<M>>,
}

pub fn right_click_menu<M: ManagedView>(id: impl Into<ElementId>) -> RightClickMenu<M> {
    RightClickMenu {
        id: id.into(),
        trigger: None,
        menu: None,
    }
}

impl<M: ManagedView> RightClickMenu<M> {
    /// The wrapped element; the flag says whether the menu is open.
    pub fn trigger<E: IntoElement + 'static>(
        mut self,
        build: impl FnOnce(bool, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.trigger = Some(Box::new(move |open, window, cx| {
            build(open, window, cx).into_any_element()
        }));
        self
    }

    pub fn menu(mut self, build: impl Fn(&mut Window, &mut App) -> Entity<M> + 'static) -> Self {
        self.menu = Some(Rc::new(build));
        self
    }
}

impl<M: ManagedView> RenderOnce for RightClickMenu<M> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| PopoverState::default());
        let (open, position) = {
            let s = state.read(cx);
            (s.open.as_ref().map(|(view, _)| view.clone()), s.position)
        };
        let trigger = self.trigger.map(|build| build(open.is_some(), window, cx));
        let builder = self.menu;
        let press_state = state.clone();

        div()
            .id(self.id)
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                let Some(build) = builder.as_ref() else {
                    return;
                };
                press_state.update(cx, |s, _| s.position = event.position);
                let menu = build(window, cx);
                show(&press_state, menu, window, cx);
            })
            .children(trigger)
            .when_some(open, |el, view| {
                el.child(
                    deferred(
                        anchored()
                            .position(position)
                            .snap_to_window_with_margin(px(8.))
                            .child(div().occlude().child(view)),
                    )
                    .with_priority(1),
                )
            })
    }
}
