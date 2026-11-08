//! Context menu at the pointer with a click-away backdrop.

use crate::app::FerrumGit;
use gpui::{AnyElement, Context, MouseButton, anchored, deferred, div, prelude::*, px};
use theme::ActiveTheme;
use ui::{Color, Label, LabelCommon, LabelSize, v_flex};

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub danger: bool,
    /// Renders a separator above this item.
    pub separator: bool,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            danger: false,
            separator: false,
        }
    }

    #[must_use]
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    #[must_use]
    pub fn sep(mut self) -> Self {
        self.separator = true;
        self
    }
}

pub fn render(app: &FerrumGit, cx: &mut Context<FerrumGit>) -> Option<AnyElement> {
    let (position, actions) = app.menu.clone()?;
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let close = |app: &mut FerrumGit, cx: &mut Context<FerrumGit>| {
        app.menu = None;
        cx.notify();
    };
    let items =
        actions.into_iter().enumerate().map(|(i, action)| {
            let item = action.item();
            v_flex()
                .when(item.separator, |d| {
                    d.child(div().my_1().h(px(1.)).bg(colors.border_variant))
                })
                .child(
                    div()
                        .id(("ctx-item", i))
                        .px_3()
                        .py_1()
                        .cursor_pointer()
                        .hover(|s| s.bg(colors.element_hover))
                        .child(Label::new(item.label).size(LabelSize::Small).color(
                            if item.danger {
                                Color::Error
                            } else {
                                Color::Default
                            },
                        ))
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.menu = None;
                            app.perform(action.clone(), window, cx);
                            cx.notify();
                        })),
                )
        });
    Some(
        div()
            .id("ctx-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, _, _, cx| close(app, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |app, _, _, cx| close(app, cx)),
            )
            .child(deferred(
                anchored().position(position).snap_to_window().child(
                    v_flex()
                        .id("ctx-menu")
                        .min_w(px(224.))
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.elevated_surface_background)
                        .shadow_lg()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .children(items),
                ),
            ))
            .into_any_element(),
    )
}
