//! A file row with change letter, name, directory and hover actions.

use crate::app::FerrumGit;
use crate::components::{change_color, split_path};
use ferrum_ui::components::{
    Color, Label, LabelCommon, LabelSize, StyledTypography, Tooltip, h_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::ChangeKind;
use gpui::{AnyElement, ClickEvent, Context, SharedString, Window, div, prelude::*, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowAction {
    pub label: &'static str,
    pub title: &'static str,
    pub danger: bool,
}

type Handler = Box<dyn Fn(&mut FerrumGit, &ClickEvent, &mut Window, &mut Context<FerrumGit>)>;

pub struct FileRow {
    pub key: String,
    pub path: String,
    pub kind: ChangeKind,
    pub active: bool,
    pub actions: Vec<(RowAction, Handler)>,
    pub on_open: Handler,
}

impl FileRow {
    pub fn new(
        key: String,
        path: String,
        kind: ChangeKind,
        active: bool,
        on_open: impl Fn(&mut FerrumGit, &ClickEvent, &mut Window, &mut Context<FerrumGit>) + 'static,
    ) -> Self {
        Self {
            key,
            path,
            kind,
            active,
            actions: Vec::new(),
            on_open: Box::new(on_open),
        }
    }

    pub fn action(
        mut self,
        action: RowAction,
        handler: impl Fn(&mut FerrumGit, &ClickEvent, &mut Window, &mut Context<FerrumGit>) + 'static,
    ) -> Self {
        self.actions.push((action, Box::new(handler)));
        self
    }

    pub fn render(self, cx: &mut Context<FerrumGit>) -> AnyElement {
        let theme = cx.theme().clone();
        let colors = theme.colors();
        let (name, dir) = split_path(&self.path);
        let (name, dir) = (name.to_owned(), dir.to_owned());
        let kind_color = change_color(self.kind, cx);
        let group = SharedString::from(format!("file-{}", self.key));
        let on_open = self.on_open;
        let buttons: Vec<AnyElement> = self
            .actions
            .into_iter()
            .enumerate()
            .map(|(i, (action, handler))| {
                div()
                    .id(SharedString::from(format!("{}-act-{i}", self.key)))
                    .flex_none()
                    .px_1p5()
                    .rounded_sm()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.editor_background)
                    .invisible()
                    .group_hover(group.clone(), |s| s.visible())
                    .hover(|s| s.bg(colors.element_hover))
                    .tooltip(Tooltip::text(action.title))
                    .child(Label::new(action.label).size(LabelSize::XSmall).color(
                        if action.danger {
                            Color::Error
                        } else {
                            Color::Default
                        },
                    ))
                    .on_click(cx.listener(move |app, event, window, cx| {
                        cx.stop_propagation();
                        handler(app, event, window, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        h_flex()
            .id(SharedString::from(self.key.clone()))
            .group(group.clone())
            .h(px(28.))
            .gap_2()
            .px_2()
            .rounded_sm()
            .cursor_pointer()
            .text_ui_xs(cx)
            .when(self.active, |d| d.bg(colors.element_selected))
            .when(!self.active, |d| d.hover(|s| s.bg(colors.element_hover)))
            .tooltip(Tooltip::text(self.path.clone()))
            .on_click(cx.listener(move |app, event, window, cx| on_open(app, event, window, cx)))
            .child(
                div()
                    .w(px(12.))
                    .flex_none()
                    .font_buffer(cx)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(kind_color)
                    .child(self.kind.letter().to_string()),
            )
            .child(div().flex_none().truncate().child(name))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(colors.text_muted)
                    .child(dir),
            )
            .children(buttons)
            .into_any_element()
    }
}

/// Section header with count and trailing buttons.
pub fn list_header(
    title: &str,
    count: usize,
    buttons: Vec<AnyElement>,
    cx: &Context<FerrumGit>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    h_flex()
        .flex_none()
        .h(px(32.))
        .gap_1()
        .px_3()
        .border_b_1()
        .border_color(colors.border)
        .bg(colors.panel_background)
        .child(
            div().flex_1().child(
                Label::new(format!("{} ({count})", title.to_uppercase()))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            ),
        )
        .children(buttons)
        .into_any_element()
}
