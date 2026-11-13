//! Layout: project bar (desktop window), sidebar beside the terminal (or
//! over it in compact mode), tab bar, the painted panes, message strip and
//! dialogs.

use super::{MESSAGE_TTL, TermView};
use crate::paint::{CellMetrics, paint_terminal, update_grid};
use ferrum_ui::components::{StyledTypography, h_flex, v_flex};
use ferrum_ui::gpui::{
    self, Axis, Context, ElementId, Hsla, MouseButton, SharedString, Window, canvas, div,
    prelude::*, px,
};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{ToggleProjectSwitcher, splitter};
use ferrumterm_protocol::{ProgramState, WindowInfo};

const TAB_BAR_HEIGHT: f32 = 34.0;
const COMPACT_TAB_BAR_HEIGHT: f32 = 28.0;
/// Widest sidebar drawn over the terminal in compact mode.
const COMPACT_SIDEBAR: f32 = 260.0;

/// A clickable tab-bar element.
#[derive(Debug, Clone, PartialEq)]
pub enum TabKind {
    Session,
    Window(String),
    NewWindow,
}

/// Sizes of the tab bar's parts: (bar height, tab padding, tab min width,
/// label length).
fn tab_sizes(compact: bool) -> (f32, f32, f32, usize) {
    if compact {
        (COMPACT_TAB_BAR_HEIGHT, 8.0, 64.0, 16)
    } else {
        (TAB_BAR_HEIGHT, 14.0, 110.0, 28)
    }
}

impl TermView {
    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let compact = self.options.compact;
        let (height, _, min_width, label_len) = tab_sizes(compact);
        let bar = h_flex()
            .flex_none()
            .h(px(height))
            .w_full()
            .px_2()
            .gap(px(2.))
            .bg(colors.tab_bar_background)
            .border_b_1()
            .border_color(colors.border)
            .font_ui(cx)
            .text_size(px(if compact { 12. } else { 13. }))
            .child(self.sidebar_toggle(cx));
        let mut tabs = Vec::new();
        if let Some(session) = self.app.session() {
            let name: String = if compact {
                session.name.chars().take(12).collect()
            } else {
                session.name.clone()
            };
            tabs.push(
                self.tab(
                    "session",
                    format!("◆ {name}"),
                    false,
                    colors.text,
                    0.0,
                    TabKind::Session,
                    cx,
                )
                .mr(px(6.)),
            );
            for window in &session.windows {
                let active = window.id == session.active_window;
                tabs.push(self.tab(
                    ElementId::Name(format!("window-{}", window.id).into()),
                    tab_label(window, label_len),
                    active,
                    if active {
                        colors.text
                    } else {
                        colors.text_muted
                    },
                    min_width,
                    TabKind::Window(window.id.to_string()),
                    cx,
                ));
            }
            tabs.push(self.tab(
                "new-window",
                "+".to_owned(),
                false,
                colors.text_muted,
                32.0,
                TabKind::NewWindow,
                cx,
            ));
        }
        if compact {
            // Tabs keep their size and scroll sideways in a narrow pane.
            bar.child(
                h_flex()
                    .id("tabs")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .gap(px(2.))
                    .overflow_x_scroll()
                    .children(tabs.into_iter().map(|tab| tab.flex_none())),
            )
        } else {
            bar.children(tabs)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tab(
        &self,
        id: impl Into<ElementId>,
        label: String,
        active: bool,
        color: Hsla,
        min_width: f32,
        kind: TabKind,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let colors = cx.theme().colors().clone();
        let (_, padding, _, _) = tab_sizes(self.options.compact);
        div()
            .id(id)
            .h_full()
            .px(px(padding))
            .min_w(px(min_width))
            .flex()
            .items_center()
            .justify_center()
            .text_color(color)
            .cursor_pointer()
            .hover(|style| style.bg(colors.ghost_element_hover))
            .when(active, |tab| {
                tab.bg(colors.tab_active_background)
                    .border_b_2()
                    .border_color(colors.text_accent)
            })
            .on_click(cx.listener(move |view, _, _, cx| {
                view.app.click_tab(kind.clone());
                view.after_change(cx);
            }))
            .child(label)
    }

    /// The sidebar drawn over the terminal (compact mode).
    fn sidebar_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        div()
            .id("sidebar-overlay")
            .absolute()
            .top_0()
            .left_0()
            .bottom_0()
            .max_w(gpui::relative(0.9))
            .occlude()
            .border_r_1()
            .border_color(colors.border)
            .shadow_lg()
            .child(self.render_sidebar(self.app.ui.sidebar_width.min(COMPACT_SIDEBAR), cx))
    }
}

impl Render for TermView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = crate::settings::get();
        let font = match &settings.font_family {
            Some(family) if !family.trim().is_empty() => gpui::font(family.clone()),
            _ => ferrum_ui::theme::theme_settings(cx).buffer_font.clone(),
        };
        // Follow a font size changed in Settings while keeping ⌘+/⌘- zoom.
        if !self.options.compact && self.app.default_font_pt != settings.font_size {
            self.app.font_pt += settings.font_size - self.app.default_font_pt;
            self.app.default_font_pt = settings.font_size;
        }
        let font_key = (self.app.font_pt, font.family.clone());
        if self.font_key.as_ref() != Some(&font_key) {
            self.metrics = None;
            self.font_key = Some(font_key);
        }
        if self
            .metrics
            .is_none_or(|metrics| metrics.font_pt != self.app.font_pt)
        {
            self.metrics = Some(CellMetrics::measure(self.app.font_pt, &font, window));
        }
        if self.options.standalone {
            window.set_window_title(&self.title());
        }

        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let message = self
            .app
            .message
            .as_ref()
            .filter(|(_, shown_at)| shown_at.elapsed() < MESSAGE_TTL)
            .map(|(text, _)| SharedString::from(text.clone()));
        let entity = cx.entity();
        let compact = self.options.compact;
        let sidebar_open = self.app.ui.sidebar_open;

        let panes_area = div()
            .id("terminal")
            .size_full()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_scroll(cx.listener(Self::scroll_wheel))
            .child(
                canvas(
                    {
                        let entity = entity.clone();
                        move |bounds, _, cx| update_grid(&entity, bounds, cx)
                    },
                    move |bounds, _, window, cx| paint_terminal(&entity, &font, bounds, window, cx),
                )
                .size_full(),
            );
        let terminal = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(self.render_tab_bar(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(panes_area)
                    .when(compact && sidebar_open, |area| {
                        area.child(self.sidebar_overlay(cx))
                    }),
            );

        let dialog = self.render_dialog(cx);
        let panes = h_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .when(sidebar_open && !compact, |root| {
                root.child(self.render_sidebar(self.app.ui.sidebar_width, cx))
                    .child(splitter("sidebar", Axis::Horizontal, cx))
            })
            .child(terminal);
        let root = crate::actions::on_actions(v_flex(), cx);
        root.relative()
            .size_full()
            .min_w(px(0.))
            .key_context(crate::KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(|view, _: &ToggleProjectSwitcher, window, cx| {
                match &view.project {
                    Some(project) => project
                        .bar
                        .update(cx, |bar, cx| bar.toggle_switcher(window, cx)),
                    None => cx.propagate(),
                }
            }))
            .on_drag_move(cx.listener(Self::resize_sidebar))
            // On the root so a selection drag can end (or keep going) outside the panes.
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, _, _| {
                    if std::mem::take(&mut view.resizing_sidebar) {
                        view.app.save_ui();
                    }
                }),
            )
            .bg(colors.terminal_background)
            .text_color(colors.terminal_foreground)
            .when_some(self.project.as_ref(), |root, project| {
                root.child(project.bar.clone())
            })
            .child(panes)
            .when_some(message, |root, message| {
                root.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(28.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .bg(status.warning_background)
                        .text_color(status.warning)
                        .font_ui(cx)
                        .text_size(px(13.))
                        .truncate()
                        .child(message),
                )
            })
            .when_some(dialog, |root, dialog| root.child(dialog))
    }
}

/// Label for a tab: window name, else the focused block's title / command,
/// cut to `max` characters, after the most urgent status glyph.
pub(crate) fn tab_label(w: &WindowInfo, max: usize) -> String {
    let b = w
        .blocks
        .iter()
        .find(|b| b.id == w.focused)
        .or(w.blocks.first());
    let title = b.map(|b| {
        b.label
            .clone()
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| {
                if b.title.is_empty() {
                    b.command.first().map_or_else(
                        || "shell".into(),
                        |c| c.rsplit('/').next().unwrap_or(c).to_owned(),
                    )
                } else {
                    b.title.clone()
                }
            })
    });
    let name = if w.name.chars().all(|c| c.is_ascii_digit()) {
        title.unwrap_or_default()
    } else {
        w.name.clone()
    };
    let status = w
        .blocks
        .iter()
        .filter_map(|b| b.status.as_ref().map(|s| s.state))
        .max_by_key(|s| match s {
            ProgramState::Blocked => 4,
            ProgramState::Error => 3,
            ProgramState::Working => 2,
            ProgramState::Done => 1,
            ProgramState::Idle => 0,
        });
    let mut label: String = name.chars().take(max).collect();
    if let Some(s) = status.filter(|_| crate::settings::get().activity_in_tabs) {
        label = format!("{} {label}", s.glyph());
    }
    label
}
