//! Building blocks of the Settings window and onboarding, public so apps
//! draw their own pages in the same style: [`Section`] (a titled card of
//! [`Row`]s), [`Switch`], [`Segmented`], [`select`], [`ChoiceCard`] and the
//! theme / colour-scheme / toolbar previews.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, Hsla, IntoElement, ParentElement, RenderOnce,
    SharedString, Styled, Window, div, prelude::*, px, relative,
};
use smallvec::SmallVec;

use crate::appearance::{ColorScheme, ToolbarStyle};
use crate::components::{
    Color, ContextMenu, DropdownMenu, Headline, HeadlineSize, Icon, IconName, IconSize, Label,
    LabelCommon, LabelSize, StyledTypography, h_flex, v_flex,
};
use crate::theme::ActiveTheme;
use crate::theme::presets::ThemePreset;

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type ValueHandler<T> = Rc<dyn Fn(T, &mut Window, &mut App)>;

/// A page heading.
pub fn page_title(title: impl Into<SharedString>) -> impl IntoElement {
    Headline::new(title).size(HeadlineSize::Medium)
}

/// A titled group of rows drawn as one rounded card.
#[derive(IntoElement)]
pub struct Section {
    title: Option<SharedString>,
    detail: Option<SharedString>,
    accessory: Option<AnyElement>,
    rows: SmallVec<[AnyElement; 6]>,
    footnote: Option<SharedString>,
}

impl Section {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: Some(title.into()),
            ..Self::untitled()
        }
    }

    pub fn untitled() -> Self {
        Self {
            title: None,
            detail: None,
            accessory: None,
            rows: SmallVec::new(),
            footnote: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// An element right of the title, e.g. a shortcut hint or a button.
    pub fn accessory(mut self, element: impl IntoElement) -> Self {
        self.accessory = Some(element.into_any_element());
        self
    }

    /// Muted text under the card.
    pub fn footnote(mut self, text: impl Into<SharedString>) -> Self {
        self.footnote = Some(text.into());
        self
    }

    pub fn row(mut self, row: impl IntoElement) -> Self {
        self.rows.push(row.into_any_element());
        self
    }

    pub fn rows(mut self, rows: impl IntoIterator<Item = impl IntoElement>) -> Self {
        self.rows
            .extend(rows.into_iter().map(IntoElement::into_any_element));
        self
    }
}

impl RenderOnce for Section {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let count = self.rows.len();
        v_flex()
            .w_full()
            .gap_2()
            .when(self.title.is_some() || self.detail.is_some(), |this| {
                this.child(
                    h_flex()
                        .px_1()
                        .gap_2()
                        .justify_between()
                        .child(
                            v_flex()
                                .when_some(self.title, |this, title| {
                                    this.child(Label::new(title).weight(gpui::FontWeight::SEMIBOLD))
                                })
                                .when_some(self.detail, |this, detail| {
                                    this.child(
                                        Label::new(detail)
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                }),
                        )
                        .children(self.accessory),
                )
            })
            .when(count > 0, |this| {
                this.child(
                    v_flex()
                        .w_full()
                        .rounded_lg()
                        .border_1()
                        .border_color(colors.border_variant)
                        .bg(colors.elevated_surface_background)
                        .px_3()
                        .children(self.rows.into_iter().enumerate().map(|(i, row)| {
                            div()
                                .w_full()
                                .when(i + 1 < count, |this| {
                                    this.border_b_1().border_color(colors.border_variant)
                                })
                                .child(row)
                        })),
                )
            })
            .when_some(self.footnote, |this, note| {
                this.child(
                    div()
                        .px_1()
                        .child(Label::new(note).size(LabelSize::XSmall).color(Color::Muted)),
                )
            })
    }
}

/// One line of a [`Section`]: title and detail on the left, a control on
/// the right. Clickable rows show a chevron.
#[derive(IntoElement)]
pub struct Row {
    id: ElementId,
    title: SharedString,
    detail: Option<SharedString>,
    icon: Option<AnyElement>,
    control: Option<AnyElement>,
    on_click: Option<ClickHandler>,
    disabled: bool,
}

impl Row {
    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            detail: None,
            icon: None,
            control: None,
            on_click: None,
            disabled: false,
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.control = Some(control.into_any_element());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Dims the row (a setting that does not apply right now).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Row {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let clickable = self.on_click.is_some() && !self.disabled;
        h_flex()
            .id(self.id)
            .w_full()
            .min_h(px(44.))
            .py_2()
            .gap_3()
            .justify_between()
            .when(self.disabled, |this| this.opacity(0.5))
            .when_some(self.on_click.filter(|_| clickable), |this, handler| {
                this.cursor_pointer()
                    .hover(|s| s.bg(colors.ghost_element_hover))
                    .on_click(move |event, window, cx| handler(event, window, cx))
            })
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_3()
                    .children(self.icon)
                    .child(v_flex().min_w_0().child(Label::new(self.title)).when_some(
                        self.detail,
                        |this, detail| {
                            this.child(
                                Label::new(detail)
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                        },
                    )),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap_2()
                    .children(self.control)
                    .when(clickable, |this| {
                        this.child(
                            Icon::new(IconName::ChevronRight)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                    }),
            )
    }
}

/// An on/off switch.
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    on: bool,
    disabled: bool,
    on_change: Option<ValueHandler<bool>>,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>, on: bool) -> Self {
        Self {
            id: id.into(),
            on,
            disabled: false,
            on_change: None,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Called with the new state.
    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let track = if self.on {
            colors.text_accent
        } else {
            colors.element_active
        };
        let knob = if self.on {
            cx.theme().readable_text_on(track)
        } else {
            colors.text_muted
        };
        let next = !self.on;
        div()
            .id(self.id)
            .flex_none()
            .w(px(36.))
            .h(px(20.))
            .p(px(2.))
            .rounded_full()
            .bg(track)
            .border_1()
            .border_color(colors.border_variant)
            .flex()
            .items_center()
            .when(self.on, |this| this.justify_end())
            .when(self.disabled, |this| this.opacity(0.5))
            .when_some(
                self.on_change.filter(|_| !self.disabled),
                |this, handler| {
                    this.cursor_pointer()
                        .on_click(move |_, window, cx| handler(next, window, cx))
                },
            )
            .child(div().size(px(14.)).rounded_full().bg(knob))
    }
}

/// A row of mutually exclusive options, one highlighted.
#[derive(IntoElement)]
pub struct Segmented {
    id: ElementId,
    items: Vec<(Option<IconName>, SharedString)>,
    selected: usize,
    on_select: Option<ValueHandler<usize>>,
}

impl Segmented {
    pub fn new(id: impl Into<ElementId>, selected: usize) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            selected,
            on_select: None,
        }
    }

    pub fn item(mut self, label: impl Into<SharedString>) -> Self {
        self.items.push((None, label.into()));
        self
    }

    pub fn icon_item(mut self, icon: IconName, label: impl Into<SharedString>) -> Self {
        self.items.push((Some(icon), label.into()));
        self
    }

    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Segmented {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let colors = theme.colors();
        let id = self.id;
        h_flex()
            .flex_none()
            .p(px(2.))
            .gap(px(2.))
            .rounded_md()
            .bg(colors.element_background)
            .border_1()
            .border_color(colors.border_variant)
            .children(
                self.items
                    .into_iter()
                    .enumerate()
                    .map(|(i, (icon, label))| {
                        let selected = i == self.selected;
                        let bg = colors.text_accent;
                        let fg = if selected {
                            theme.readable_text_on(bg)
                        } else {
                            colors.text
                        };
                        let handler = self.on_select.clone();
                        h_flex()
                            .id(ElementId::NamedInteger(
                                SharedString::from(format!("{id:?}-segment")),
                                i as u64,
                            ))
                            .px_2()
                            .py(px(3.))
                            .gap_1()
                            .rounded_sm()
                            .text_ui_sm(cx)
                            .text_color(fg)
                            .when(selected, |this| this.bg(bg))
                            .when(!selected, |this| {
                                this.cursor_pointer()
                                    .hover(|s| s.bg(colors.ghost_element_hover))
                            })
                            .when_some(handler.filter(|_| !selected), |this, handler| {
                                this.on_click(move |_, window, cx| handler(i, window, cx))
                            })
                            .when_some(icon, |this, icon| {
                                this.child(
                                    Icon::new(icon)
                                        .size(IconSize::Small)
                                        .color(Color::Custom(fg)),
                                )
                            })
                            .child(label)
                    }),
            )
    }
}

/// A dropdown showing `options[selected]`; picking one calls `on_select`.
pub fn select(
    id: impl Into<ElementId>,
    options: &[&'static str],
    selected: usize,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let on_select = Rc::new(on_select);
    let label = options.get(selected).copied().unwrap_or_default();
    let options = options.to_vec();
    let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
        for (i, option) in options.into_iter().enumerate() {
            let on_select = on_select.clone();
            menu = menu.toggleable_entry(
                option,
                i == selected,
                crate::components::IconPosition::Start,
                None,
                move |window, cx| on_select(i, window, cx),
            );
        }
        menu
    });
    DropdownMenu::new(id, label, menu)
}

/// A selectable tile: a preview with a caption, outlined when selected.
#[derive(IntoElement)]
pub struct ChoiceCard {
    id: ElementId,
    preview: AnyElement,
    caption: SharedString,
    selected: bool,
    width: gpui::Pixels,
    on_click: Option<ClickHandler>,
}

impl ChoiceCard {
    pub fn new(
        id: impl Into<ElementId>,
        caption: impl Into<SharedString>,
        preview: impl IntoElement,
    ) -> Self {
        Self {
            id: id.into(),
            preview: preview.into_any_element(),
            caption: caption.into(),
            selected: false,
            width: px(96.),
            on_click: None,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn width(mut self, width: gpui::Pixels) -> Self {
        self.width = width;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for ChoiceCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let ring = if self.selected {
            colors.text_accent
        } else {
            gpui::transparent_black()
        };
        v_flex()
            .id(self.id)
            .flex_none()
            .w(self.width)
            .gap_1()
            .items_center()
            .when_some(self.on_click, |this, handler| {
                this.cursor_pointer()
                    .on_click(move |event, window, cx| handler(event, window, cx))
            })
            .child(
                div()
                    .w_full()
                    .p(px(2.))
                    .rounded_lg()
                    .border_2()
                    .border_color(ring)
                    .child(self.preview),
            )
            .child(
                Label::new(self.caption)
                    .size(LabelSize::Small)
                    .color(if self.selected {
                        Color::Default
                    } else {
                        Color::Muted
                    })
                    .when(self.selected, |l| l.weight(gpui::FontWeight::SEMIBOLD)),
            )
    }
}

/// A miniature window in the given colours, for the scheme / toolbar cards.
fn mini_window(bg: Hsla, bar: Hsla, pane: Hsla, border: Hsla) -> Div {
    let dots = h_flex().gap(px(2.)).children(
        [0xff5f57, 0xfebc2e, 0x28c840].map(|c| div().size(px(4.)).rounded_full().bg(gpui::rgb(c))),
    );
    v_flex()
        .size_full()
        .rounded_md()
        .overflow_hidden()
        .border_1()
        .border_color(border)
        .bg(bg)
        .child(h_flex().h(px(9.)).px_1().bg(bar).child(dots))
        .child(
            div()
                .flex_1()
                .p(px(3.))
                .child(div().size_full().rounded_sm().bg(pane)),
        )
}

/// Preview for a colour scheme card.
pub fn scheme_preview(scheme: ColorScheme) -> impl IntoElement {
    let light = || {
        mini_window(
            gpui::rgb(0xf4f4f5).into(),
            gpui::rgb(0xe4e4e7).into(),
            gpui::rgb(0xffffff).into(),
            gpui::rgb(0xd4d4d8).into(),
        )
    };
    let dark = || {
        mini_window(
            gpui::rgb(0x1f2228).into(),
            gpui::rgb(0x16181d).into(),
            gpui::rgb(0x2a2e36).into(),
            gpui::rgb(0x3a3f4b).into(),
        )
    };
    let frame = div().h(px(56.)).w_full().relative();
    match scheme {
        ColorScheme::Light => frame.child(light()),
        ColorScheme::Dark => frame.child(dark()),
        ColorScheme::Auto => frame.child(light()).child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(relative(0.5))
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .w(px(88.))
                        .child(dark()),
                ),
        ),
    }
}

/// Preview for a toolbar density card.
pub fn toolbar_preview(style: ToolbarStyle, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let bar = match style {
        ToolbarStyle::Comfortable => px(14.),
        ToolbarStyle::Compact => px(9.),
    };
    v_flex()
        .h(px(56.))
        .w_full()
        .rounded_md()
        .overflow_hidden()
        .border_1()
        .border_color(colors.border)
        .bg(colors.editor_background)
        .child(
            h_flex()
                .h(bar)
                .px_1()
                .gap(px(3.))
                .bg(colors.title_bar_background)
                .border_b_1()
                .border_color(colors.border)
                .children((0..3).map(|_| {
                    div()
                        .w(px(12.))
                        .h(px(4.))
                        .rounded_sm()
                        .bg(colors.text_muted)
                })),
        )
        .child(div().flex_1())
}

/// A theme tile: "Aa", a prompt chip and a cursor in the theme's colours
/// over its background, with its ANSI colours below.
pub fn theme_preview(preset: &ThemePreset) -> impl IntoElement {
    let s = preset.swatches();
    v_flex()
        .h(px(64.))
        .w_full()
        .p_2()
        .gap_1()
        .rounded_md()
        .bg(s.background)
        .child(
            h_flex()
                .gap_2()
                .child(div().text_color(s.text).text_lg().child("Aa"))
                .child(
                    div()
                        .px_1()
                        .rounded_sm()
                        .bg(s.surface)
                        .text_color(s.accent)
                        .text_sm()
                        .child("~/"),
                )
                .child(div().w(px(6.)).h(px(14.)).rounded_sm().bg(s.accent)),
        )
        .child(
            h_flex().gap(px(2.)).children(
                s.ansi
                    .into_iter()
                    .map(|c| div().size(px(9.)).rounded_full().bg(c)),
            ),
        )
}

/// A key combination drawn as separate key caps (`⇧ ⌘ P`).
pub fn key_caps(keys: &str, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    h_flex()
        .gap_1()
        .children(keys.split_whitespace().map(|key| {
            div()
                .min_w(px(22.))
                .h(px(22.))
                .px_1()
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .bg(colors.element_background)
                .border_1()
                .border_color(colors.border_variant)
                .text_ui_sm(cx)
                .text_color(colors.text_muted)
                .child(SharedString::from(key.to_owned()))
        }))
}

/// A rounded square holding an icon, for page and card headers.
pub fn icon_tile(icon: IconName, tint: Hsla, size: gpui::Pixels) -> impl IntoElement {
    div()
        .flex_none()
        .size(size)
        .rounded_lg()
        .bg(tint.opacity(0.18))
        .border_1()
        .border_color(tint.opacity(0.35))
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::new(icon)
                .size(if size > px(40.) {
                    IconSize::XLarge
                } else {
                    IconSize::Small
                })
                .color(Color::Custom(tint)),
        )
}
