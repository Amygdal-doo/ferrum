//! Headless GPUI tests: every component renders in both built-in themes,
//! and context menus run entries from the keyboard.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    Context, DismissEvent, Entity, Focusable, IntoElement, Modifiers, ParentElement, Render,
    Styled, TestAppContext, Window, div, point, px,
};

use super::*;
use crate::menu::{Confirm, SelectNext};
use crate::theme::{self, GlobalTheme};

/// One of everything.
struct Showcase;

impl Render for Showcase {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu = ContextMenu::build(window, cx, |menu, _, _| {
            menu.header("Pick")
                .toggleable_entry("One", true, IconPosition::Start, None, |_, _| {})
                .separator()
                .entry("Two", Some(Box::new(SelectNext)), |_, _| {})
        });
        let styles = [
            ButtonStyle::Filled,
            ButtonStyle::Outlined,
            ButtonStyle::OutlinedGhost,
            ButtonStyle::Subtle,
            ButtonStyle::Transparent,
            ButtonStyle::Tinted(TintColor::Error),
        ];
        v_flex()
            .size_full()
            .gap_2()
            .child(Headline::new("Showcase").size(HeadlineSize::Large))
            .child(
                h_flex().children(styles.into_iter().enumerate().map(|(i, style)| {
                    Button::new(("button", i), "Run")
                        .style(style)
                        .size(ButtonSize::Compact)
                        .start_icon(Icon::new(IconName::PlayFilled).size(IconSize::XSmall))
                        .toggle_state(i == 0)
                        .disabled(i == 4)
                        .tooltip(Tooltip::text("Run it"))
                })),
            )
            .child(
                IconButton::new("close", IconName::Close)
                    .icon_size(IconSize::Small)
                    .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                    .toggle_state(true),
            )
            .child(
                Label::new("muted\nsingle line")
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .single_line()
                    .strikethrough(),
            )
            .child(LoadingLabel::new("Loading"))
            .child(Checkbox::new("on", ToggleState::Selected).label("On"))
            .child(Checkbox::new("mixed", ToggleState::Indeterminate).disabled(true))
            .child(Divider::horizontal())
            .child(ProgressBar::new("progress", 0.4, 1., cx))
            .child(
                TabBar::new("tabs")
                    .child(Tab::new("a").toggle_state(true).child(Label::new("A")))
                    .child(Tab::new("b").child(Label::new("B")))
                    .end_child(IconButton::new("add", IconName::Plus)),
            )
            .child(
                ListItem::new("row")
                    .rounded()
                    .indent_level(2)
                    .toggle_state(true)
                    .start_slot(Icon::new(IconName::Folder))
                    .child(Label::new("src"))
                    .end_hover_slot(IconButton::new("more", IconName::Ellipsis)),
            )
            .child(DropdownMenu::new("dropdown", "One", menu).style(DropdownStyle::Outlined))
            .child(
                PopoverMenu::new("popover")
                    .trigger(IconButton::new("open", IconName::ChevronDown))
                    .menu(|window, cx| Some(ContextMenu::build(window, cx, |m, _, _| m))),
            )
            .child(
                right_click_menu("context")
                    .trigger(|_, _, _| div().child("right-click me"))
                    .menu(|window, cx| ContextMenu::build(window, cx, |m, _, _| m)),
            )
    }
}

#[gpui::test]
fn every_component_renders_in_both_themes(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    let (_view, cx) = cx.add_window_view(|_, _| Showcase);
    cx.run_until_parked();
    cx.update(|window, cx| {
        GlobalTheme::update_theme(cx, Arc::new(theme::light()));
        window.refresh();
    });
    cx.run_until_parked();
}

#[gpui::test]
fn context_menu_runs_the_selected_entry(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    let ran = Rc::new(Cell::new(0));
    let dismissed = Rc::new(Cell::new(false));
    let (host, cx) = cx.add_window_view({
        let ran = ran.clone();
        move |window, cx| {
            Host(ContextMenu::build(window, cx, move |menu, _, _| {
                menu.header("Actions")
                    .entry("Count", None, move |_, _| ran.set(ran.get() + 1))
            }))
        }
    });
    let menu = host.read_with(cx, |host, _| host.0.clone());
    cx.update({
        let dismissed = dismissed.clone();
        move |window, cx| {
            cx.subscribe(&menu, move |_, _: &DismissEvent, _| dismissed.set(true))
                .detach();
            window.focus(&menu.focus_handle(cx), cx);
        }
    });
    cx.run_until_parked();
    cx.dispatch_action(SelectNext);
    cx.dispatch_action(Confirm);
    cx.run_until_parked();
    assert_eq!(ran.get(), 1);
    assert!(dismissed.get());
}

/// Hosts a menu entity as the window root.
struct Host(Entity<ContextMenu>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone())
    }
}

/// A popover trigger alone in the top-left corner of the window.
struct PopoverHost(Rc<Cell<usize>>);

impl Render for PopoverHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let builds = self.0.clone();
        div().size_full().child(
            PopoverMenu::new("menu")
                .trigger(IconButton::new("open", IconName::Ellipsis))
                .menu(move |window, cx| {
                    builds.set(builds.get() + 1);
                    Some(ContextMenu::build(window, cx, |m, _, _| {
                        m.entry("Only", None, |_, _| {})
                    }))
                }),
        )
    }
}

#[gpui::test]
fn popover_opens_on_click_and_closes_on_escape(cx: &mut TestAppContext) {
    cx.update(crate::init_for_tests);
    let builds = Rc::new(Cell::new(0));
    let (_host, cx) = cx.add_window_view({
        let builds = builds.clone();
        move |_, _| PopoverHost(builds)
    });
    cx.update(|window, cx| {
        cx.bind_keys([gpui::KeyBinding::new("escape", crate::menu::Cancel, None)]);
        window.refresh();
    });
    cx.run_until_parked();
    let trigger = point(px(8.), px(8.));

    cx.simulate_click(trigger, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(builds.get(), 1, "first click opens the menu");
    let focused_menu = cx.update(|window, cx| window.focused(cx).is_some());
    assert!(focused_menu, "the open menu takes focus");

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_click(trigger, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(builds.get(), 2, "after Escape a click opens it again");

    // Clicking the trigger of an open menu closes it without reopening.
    cx.simulate_click(trigger, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(builds.get(), 2);
}

fn sample_menu(cx: &mut gpui::TestAppContext) -> Entity<ContextMenu> {
    let window = cx.add_empty_window();
    window.update(|window, cx| {
        ContextMenu::build(window, cx, |menu, _, _| {
            menu.header("Group")
                .entry("One", None, |_, _| {})
                .separator()
                .disabled_entry("Off")
                .entry("Two", None, |_, _| {})
        })
    })
}

#[gpui::test]
fn keyboard_selection_skips_unselectable_items(cx: &mut gpui::TestAppContext) {
    let menu = sample_menu(cx);
    menu.update(cx, |menu, cx| {
        menu.step(true, cx);
        assert_eq!(menu.selected_index(), Some(1));
        menu.step(true, cx);
        assert_eq!(menu.selected_index(), Some(4));
        menu.step(true, cx);
        assert_eq!(menu.selected_index(), Some(1), "wraps around");
        menu.step(false, cx);
        assert_eq!(menu.selected_index(), Some(4));
        menu.select_edge(false, cx);
        assert_eq!(menu.selected_index(), Some(1));
    });
}
