use gpui::{
    AnyElement, App, ClickEvent, IntoElement, Pixels, SharedString, Window, div, hsla, prelude::*,
    px,
};
use smallvec::SmallVec;
use theme::ActiveTheme;
use ui::{Clickable, Headline, HeadlineSize, IconButton, IconName, h_flex, v_flex};

type DismissHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A centered dialog over a dimmed backdrop. Render it as the last child of
/// the root view (which must be `relative`) while the dialog is open.
#[derive(IntoElement)]
pub struct Modal {
    title: SharedString,
    width: Pixels,
    body: SmallVec<[AnyElement; 2]>,
    footer: SmallVec<[AnyElement; 2]>,
    on_dismiss: Option<DismissHandler>,
    dismiss_on_backdrop: bool,
}

impl Modal {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            width: px(480.),
            body: SmallVec::new(),
            footer: SmallVec::new(),
            on_dismiss: None,
            dismiss_on_backdrop: false,
        }
    }

    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    /// Adds a button (or any element) to the right-aligned footer row.
    pub fn footer(mut self, element: impl IntoElement) -> Self {
        self.footer.push(element.into_any_element());
        self
    }

    /// Called for the close button (and backdrop clicks when
    /// [`Modal::dismiss_on_backdrop`] is set).
    pub fn on_dismiss(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Box::new(handler));
        self
    }

    /// Off by default so a stray click can't discard a half-filled form.
    pub fn dismiss_on_backdrop(mut self, enabled: bool) -> Self {
        self.dismiss_on_backdrop = enabled;
        self
    }
}

impl ParentElement for Modal {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.body.extend(elements)
    }
}

impl RenderOnce for Modal {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let on_dismiss = self.on_dismiss.map(std::rc::Rc::new);

        div()
            .id("ferrum-modal-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(hsla(0., 0., 0., 0.45))
            .occlude()
            .when_some(
                on_dismiss.clone().filter(|_| self.dismiss_on_backdrop),
                |this, on_dismiss| this.on_click(move |event, window, cx| on_dismiss(event, window, cx)),
            )
            .child(
                v_flex()
                    .id("ferrum-modal")
                    .w(self.width)
                    .max_h(gpui::relative(0.85))
                    .rounded_lg()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.elevated_surface_background)
                    .shadow_lg()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        h_flex()
                            .justify_between()
                            .px_4()
                            .py_3()
                            .border_b_1()
                            .border_color(colors.border_variant)
                            .child(Headline::new(self.title).size(HeadlineSize::XSmall))
                            .when_some(on_dismiss, |this, on_dismiss| {
                                this.child(
                                    IconButton::new("ferrum-modal-close", IconName::Close)
                                        .on_click(move |event, window, cx| {
                                            on_dismiss(event, window, cx)
                                        }),
                                )
                            }),
                    )
                    .child(
                        v_flex()
                            .id("ferrum-modal-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .gap_3()
                            .p_4()
                            .children(self.body),
                    )
                    .when(!self.footer.is_empty(), |this| {
                        this.child(
                            h_flex()
                                .justify_end()
                                .gap_2()
                                .px_4()
                                .py_3()
                                .border_t_1()
                                .border_color(colors.border_variant)
                                .children(self.footer),
                        )
                    }),
            )
    }
}
