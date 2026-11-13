//! The Settings window: a searchable sidebar of pages, back / forward
//! history and the selected page.

use std::rc::Rc;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, Focusable, Global, IntoElement, ParentElement,
    Render, Styled, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div,
    prelude::*, px, size,
};

use super::widgets::{icon_tile, page_title};
use super::{SettingsPage, shared_pages};
use crate::app_info::app_info;
use crate::components::{
    Clickable, Color, Disableable, IconButton, IconName, Label, LabelCommon, LabelSize, h_flex,
    v_flex,
};
use crate::text_input::{TextInput, TextInputEvent};
use crate::theme::ActiveTheme;

pub struct SettingsWindow {
    pages: Vec<Rc<dyn SettingsPage>>,
    current: usize,
    back: Vec<usize>,
    forward: Vec<usize>,
    search: Entity<TextInput>,
    focus: FocusHandle,
    _subscription: gpui::Subscription,
}

struct OpenSettingsWindow(WindowHandle<SettingsWindow>);

impl Global for OpenSettingsWindow {}

/// Opens Settings (or brings it forward), on page `page` when given.
pub fn open_settings(page: Option<&str>, cx: &mut App) {
    let page = page.map(str::to_owned);
    if let Some(handle) = cx.try_global::<OpenSettingsWindow>().map(|g| g.0)
        && handle
            .update(cx, |settings, window, cx| {
                if let Some(page) = &page {
                    settings.show(page, cx);
                }
                window.activate_window();
            })
            .is_ok()
    {
        return;
    }
    let info = app_info(cx);
    let bounds = Bounds::centered(None, size(px(920.), px(680.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(720.), px(480.))),
        titlebar: Some(TitlebarOptions {
            title: Some(format!("{} Settings", info.name).into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| {
        cx.new(|cx| {
            let mut settings = SettingsWindow::new(window, cx);
            if let Some(page) = &page {
                settings.show(page, cx);
            }
            settings
        })
    }) {
        Ok(handle) => cx.set_global(OpenSettingsWindow(handle)),
        Err(error) => eprintln!("ferrum: could not open Settings: {error:#}"),
    }
}

impl SettingsWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (head, tail) = shared_pages();
        let mut pages = head;
        pages.extend(app_info(cx).pages.iter().cloned());
        pages.extend(tail);
        let search = cx.new(|cx| TextInput::new(window, cx).placeholder("Search Settings"));
        let subscription = cx.subscribe(&search, |this: &mut Self, _, event, cx| {
            if *event == TextInputEvent::Changed {
                if let Some(first) = this.visible(cx).first().copied()
                    && !this.visible(cx).contains(&this.current)
                {
                    this.go(first, cx);
                }
                cx.notify();
            }
        });
        Self {
            pages,
            current: 0,
            back: Vec::new(),
            forward: Vec::new(),
            search,
            focus: cx.focus_handle(),
            _subscription: subscription,
        }
    }

    /// Navigates to the page with `id`.
    pub fn show(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.pages.iter().position(|p| p.id() == id) {
            self.go(index, cx);
        }
    }

    fn go(&mut self, index: usize, cx: &mut Context<Self>) {
        if index != self.current {
            self.back.push(self.current);
            self.forward.clear();
            self.current = index;
            cx.notify();
        }
    }

    fn step(&mut self, back: bool, cx: &mut Context<Self>) {
        let (from, to) = if back {
            (&mut self.back, &mut self.forward)
        } else {
            (&mut self.forward, &mut self.back)
        };
        if let Some(index) = from.pop() {
            to.push(self.current);
            self.current = index;
            cx.notify();
        }
    }

    /// Indexes of the pages matching the search text.
    fn visible(&self, cx: &App) -> Vec<usize> {
        let query = self.search.read(cx).text().trim().to_lowercase();
        self.pages
            .iter()
            .enumerate()
            .filter(|(_, page)| {
                query.is_empty()
                    || page.title().to_lowercase().contains(&query)
                    || page.keywords().iter().any(|k| k.contains(&query))
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let visible = self.visible(cx);
        // Gaps between the look, the app's own and the general pages.
        let app_pages = app_info(cx).pages.len();
        let breaks = [2, 2 + app_pages];
        v_flex()
            .flex_none()
            .w(px(230.))
            .h_full()
            .p_3()
            .gap_1()
            .bg(colors.panel_background)
            .border_r_1()
            .border_color(colors.border)
            .child(div().pb_2().child(self.search.clone()))
            .children(visible.into_iter().map(|index| {
                let page = &self.pages[index];
                let selected = index == self.current;
                div()
                    .when(breaks.contains(&index) && index > 0, |this| this.mt_3())
                    .child(
                        h_flex()
                            .id(page.id())
                            .px_2()
                            .py_1p5()
                            .gap_2()
                            .rounded_md()
                            .cursor_pointer()
                            .when(selected, |this| this.bg(colors.element_selected))
                            .when(!selected, |this| {
                                this.hover(|s| s.bg(colors.ghost_element_hover))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| this.go(index, cx)))
                            .child(icon_tile(page.icon(), page.tint(), px(24.)))
                            .child(
                                Label::new(page.title())
                                    .when(selected, |l| l.weight(gpui::FontWeight::SEMIBOLD)),
                            ),
                    )
            }))
    }
}

impl Focusable for SettingsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(crate::appearance::rem_size(cx));
        let colors = cx.theme().colors().clone();
        let page = self.pages[self.current].clone();
        h_flex()
            .size_full()
            .track_focus(&self.focus)
            .bg(colors.background)
            .text_color(colors.text)
            .child(self.render_sidebar(cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        h_flex()
                            .flex_none()
                            .h(px(52.))
                            .px_4()
                            .gap_3()
                            .border_b_1()
                            .border_color(colors.border_variant)
                            .child(
                                h_flex()
                                    .child(
                                        IconButton::new("settings-back", IconName::ChevronLeft)
                                            .disabled(self.back.is_empty())
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.step(true, cx)),
                                            ),
                                    )
                                    .child(
                                        IconButton::new("settings-forward", IconName::ChevronRight)
                                            .disabled(self.forward.is_empty())
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.step(false, cx)),
                                            ),
                                    ),
                            )
                            .child(page_title(page.title())),
                    )
                    .child(
                        v_flex()
                            .id(("settings-page", self.current))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(
                                v_flex()
                                    .w_full()
                                    .max_w(px(760.))
                                    .px_6()
                                    .py_5()
                                    .gap_6()
                                    .child(page.render(window, cx)),
                            ),
                    ),
            )
            .when(self.visible(cx).is_empty(), |this| {
                this.child(
                    Label::new("No settings match")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
    }
}
