//! Layout: project bar (desktop window), sidebar beside the terminal (or
//! over it in compact mode), tab bar, the painted panes, message strip and
//! dialogs.

use super::{MESSAGE_TTL, TermView};
use crate::paint::{CellMetrics, paint_terminal, update_grid};
use ferrum_ui::ToggleProjectSwitcher;
use ferrum_ui::components::{Clickable, StyledTypography, h_flex, v_flex};
use ferrum_ui::gpui::{
    self, Axis, Context, ElementId, Hsla, MouseButton, SharedString, Window, canvas, div,
    prelude::*, px,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumterm_protocol::{ProgramState, WindowInfo};

const TAB_BAR_HEIGHT: f32 = 34.0;
const COMPACT_TAB_BAR_HEIGHT: f32 = 28.0;
/// Widest sidebar drawn over the terminal in compact mode.
const COMPACT_SIDEBAR: f32 = 260.0;

/// A clickable tab-bar element.
#[derive(Debug, Clone, PartialEq)]
pub enum TabKind {
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
        let cards = !compact;
        let bar = h_flex()
            .flex_none()
            .h(px(height))
            .w_full()
            .px_2()
            .gap(px(2.))
            .when(!cards, |bar| bar.bg(colors.tab_bar_background))
            .border_b_1()
            .border_color(if cards {
                colors.border_variant
            } else {
                colors.border
            })
            .font_ui(cx)
            .text_size(px(if compact { 12. } else { 13. }))
            .child(self.sidebar_toggle(cx));
        let mut tabs = Vec::new();
        let in_sidebar =
            crate::settings::get().tab_position == crate::settings::TabPosition::Sidebar;
        if in_sidebar {
            // Tabs are listed in the sidebar: show only the current one.
            if let Some(w) = self.app.window() {
                tabs.push(
                    h_flex()
                        .id("current-tab")
                        .h_full()
                        .gap_1()
                        .pr_2()
                        .child(self.tab_chip(w, cx))
                        .child(
                            div()
                                .truncate()
                                .text_color(colors.text)
                                .child(tab_label(w, label_len)),
                        ),
                );
            }
        } else if let Some(session) = self.app.session() {
            for window in &session.windows {
                let active = window.id == session.active_window;
                let chip = self.tab_chip(window, cx);
                tabs.push(
                    h_flex()
                        .h_full()
                        .gap(px(1.))
                        .child(chip)
                        .child(self.tab(
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
                        ))
                        .id(ElementId::Name(format!("tab-group-{}", window.id).into())),
                );
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
            .when(!self.options.compact, |tab| tab.my(px(5.)).rounded_md())
            .when(active && self.options.compact, |tab| {
                tab.bg(colors.tab_active_background)
                    .border_b_2()
                    .border_color(colors.text_accent)
            })
            .when(active && !self.options.compact, |tab| {
                tab.bg(colors.element_selected)
                    .border_1()
                    .border_color(colors.border_variant)
                    .shadow_sm()
            })
            .on_click(cx.listener(move |view, _, _, cx| {
                view.app.click_tab(kind.clone());
                view.after_change(cx);
            }))
            .child(label)
    }

    /// Shown instead of the terminals while an older server holds the socket.
    fn render_incompatible(&self, why: String, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(colors.terminal_background)
            .font_ui(cx)
            .child(
                v_flex()
                    .max_w(px(520.))
                    .p_5()
                    .gap_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(colors.border_variant)
                    .bg(colors.elevated_surface_background)
                    .text_color(colors.text)
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("An older FerrumTerm server is running"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(colors.text_muted)
                            .child(if why.starts_with("could not restart") {
                                why
                            } else {
                                "It was started by a previous FerrumTerm build and can't be \
                                 used by this one. Restarting stops it; programs still running \
                                 in it are closed."
                                    .to_owned()
                            }),
                    )
                    .child(
                        h_flex().justify_end().child(
                            div()
                                .id("restart-server")
                                .px_3()
                                .py_1p5()
                                .rounded_md()
                                .cursor_pointer()
                                .bg(colors.text_accent)
                                .text_color(cx.theme().readable_text_on(colors.text_accent))
                                .text_size(px(13.))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child("Restart server")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.incompatible = None;
                                    view.app.notify("Restarting the FerrumTerm server…");
                                    view.app.restart_server();
                                    cx.notify();
                                })),
                        ),
                    ),
            )
    }

    /// The button before a tab's name: its icon in its colour; click it to
    /// pick another (the tab itself keeps the normal background).
    pub(crate) fn tab_chip(&self, window: &WindowInfo, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::tab_style::{COLORS, ICONS, TabStyle};
        use ferrum_ui::components::{
            ButtonCommon, Color, ContentButton, ContextMenu, Icon, IconPosition, IconSize,
            PopoverMenu, Tooltip,
        };
        let window_id = window.id.to_string();
        let custom = self.app.ui.tab_styles.get(&window_id).cloned();
        let style = custom.clone().unwrap_or_else(|| TabStyle::auto(window));
        // Unstyled tabs show the agent's logo, or the plain `>_` tile.
        let mark = match (&custom, TabStyle::agent_logo(window)) {
            (Some(style), _) => Icon::new(style.icon_name())
                .size(IconSize::Small)
                .color(Color::Custom(style.hsla()))
                .into_any_element(),
            (None, Some(logo)) => ferrum_ui::agent_logo(logo, px(15.), cx).into_any_element(),
            (None, None) => ferrum_ui::terminal_tile(px(17.), cx).into_any_element(),
        };
        let weak = cx.weak_entity();
        let id = window_id;
        PopoverMenu::new(SharedString::from(format!("chip-{id}")))
            .trigger(
                ContentButton::new(SharedString::from(format!("chip-button-{id}")), mark)
                    .tooltip(Tooltip::text("Tab icon and colour")),
            )
            .menu(move |window, cx| {
                let weak = weak.clone();
                let id = id.clone();
                let current = style.clone();
                Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                    let current = current.clone();
                    let set = |icon: Option<&'static str>, color: Option<&'static str>| {
                        let weak = weak.clone();
                        let id = id.clone();
                        let current = current.clone();
                        move |_: &mut Window, cx: &mut gpui::App| {
                            let _ = weak.update(cx, |view, cx| {
                                let style = view
                                    .app
                                    .ui
                                    .tab_styles
                                    .entry(id.clone())
                                    .or_insert_with(|| current.clone());
                                if let Some(icon) = icon {
                                    style.icon = icon.to_owned();
                                }
                                if let Some(color) = color {
                                    style.color = color.to_owned();
                                }
                                view.app.ui.save_recent_agents();
                                cx.notify();
                            });
                        }
                    };
                    menu = menu.header("Icon");
                    for (key, label, icon) in ICONS {
                        menu = menu.icon_entry(label, icon, set(Some(key), None));
                    }
                    menu = menu.header("Colour");
                    for (key, label, _) in COLORS {
                        menu = menu.toggleable_entry(
                            label,
                            current.color == key,
                            IconPosition::Start,
                            None,
                            set(None, Some(key)),
                        );
                    }
                    let weak = weak.clone();
                    let id = id.clone();
                    menu.separator().entry("Reset", None, move |_, cx| {
                        let _ = weak.update(cx, |view, cx| {
                            view.app.ui.tab_styles.remove(&id);
                            view.app.ui.save_recent_agents();
                            cx.notify();
                        });
                    })
                }))
            })
    }

    /// ◧, ⌘ and + : in the sidebar's title row, or at the start of the
    /// terminal's header while the sidebar is hidden.
    fn title_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(self.sidebar_toggle(cx))
            .when(self.project.is_some(), |row| {
                row.child(
                    ferrum_ui::command_button("switch-project", "Switch project (⌥⌘O)").on_click(
                        cx.listener(|view, _, window, cx| {
                            if let Some(project) = &view.project {
                                project
                                    .bar
                                    .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
                            }
                        }),
                    ),
                )
            })
            .child(self.new_menu("new-menu", cx))
    }

    /// The window's sidebar: title row (traffic lights, ◧ ⌘ +), project,
    /// then tabs, agents and hosts.
    fn sidebar_column(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = self.app.ui.sidebar_width;
        v_flex()
            .flex_none()
            .w(px(width))
            .h_full()
            .child(
                ferrum_ui::title_row("sidebar-title", true, window)
                    .child(div().flex_1())
                    .child(self.title_controls(cx)),
            )
            .when_some(self.project.as_ref(), |col, project| {
                col.child(div().px_1().child(project.bar.clone()))
            })
            .child(self.render_sidebar(width, cx))
    }

    /// The terminal card's header: the tabs (or, with tabs in the sidebar,
    /// the active tab's icon and name), and while the sidebar is hidden the
    /// window controls and project.
    fn render_pane_header(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let sidebar_open = self.app.ui.sidebar_open;
        let in_sidebar =
            crate::settings::get().tab_position == crate::settings::TabPosition::Sidebar;
        let mut row = ferrum_ui::title_row("pane-header", !sidebar_open, window)
            .gap_1()
            .border_b_1()
            .border_color(colors.border_variant)
            .font_ui(cx)
            .text_size(px(13.));
        if !sidebar_open {
            row = row.child(self.title_controls(cx)).child(div().w(px(6.)));
        }
        if in_sidebar {
            if let Some(w) = self.app.window() {
                row = row.child(self.tab_chip(w, cx)).child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(colors.text)
                        .child(tab_label(w, 60)),
                );
            }
        } else if let Some(session) = self.app.session() {
            let mut tabs = h_flex()
                .id("tabs")
                .flex_1()
                .min_w_0()
                .gap_1()
                .overflow_x_scroll();
            for w in &session.windows {
                let active = w.id == session.active_window;
                let kind = TabKind::Window(w.id.to_string());
                tabs = tabs.child(
                    ferrum_ui::header_tab(
                        ElementId::Name(format!("window-{}", w.id).into()),
                        active,
                        cx,
                    )
                    .pl_0p5()
                    // No fill: the active tab stands out by its brighter title.
                    .bg(gpui::transparent_black())
                    .child(self.tab_chip(w, cx))
                    .child(tab_label(w, 28))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.app.click_tab(kind.clone());
                        view.after_change(cx);
                    })),
                );
            }
            tabs = tabs.child(
                ferrum_ui::title_button(
                    "new-window",
                    ferrum_ui::components::IconName::Plus,
                    "New tab (⌘T)",
                )
                .on_click(cx.listener(|view, _, _, cx| {
                    view.app.click_tab(TabKind::NewWindow);
                    view.after_change(cx);
                })),
            );
            row = row.child(tabs);
        }
        row.child(div().flex_1()).when(!sidebar_open, |row| {
            row.when_some(self.project.as_ref(), |row, project| {
                row.child(div().flex_none().max_w(px(320.)).child(project.bar.clone()))
            })
        })
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
            .bg(colors.panel_background)
            .child(
                v_flex()
                    .h_full()
                    .child(
                        h_flex()
                            .flex_none()
                            .h(px(32.))
                            .px_1()
                            .child(div().flex_1())
                            .child(self.new_menu("new-menu", cx)),
                    )
                    .child(self.render_sidebar(self.app.ui.sidebar_width.min(COMPACT_SIDEBAR), cx)),
            )
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
        // The window: sidebar on the backdrop, the terminal in a card (lighter
        // while focused). Embedded panes stay flat (the host frames them).
        let cards = !compact;
        let terminal_area = div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(colors.terminal_background)
            .child(panes_area)
            .when_some(self.incompatible.clone(), |area, why| {
                area.child(self.render_incompatible(why, cx))
            })
            .when(compact && sidebar_open, |area| {
                area.child(self.sidebar_overlay(cx))
            });
        let terminal = if cards {
            let active = self.focus_handle.contains_focused(window, cx);
            ferrum_ui::pane_card(active, cx)
                .flex_1()
                .h_full()
                .child(self.render_pane_header(window, cx))
                .child(terminal_area)
        } else {
            v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(self.render_tab_bar(cx))
                .child(terminal_area)
        };

        let dialog = self.render_dialog(cx);
        let panes = h_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .when(sidebar_open && cards, |root| {
                root.child(self.sidebar_column(window, cx))
                    .child(ferrum_ui::gap_splitter("sidebar", Axis::Horizontal, cx))
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .when(cards, |main| {
                        main.py(ferrum_ui::PANE_GAP)
                            .pr(ferrum_ui::PANE_GAP)
                            .when(!sidebar_open, |main| main.pl(ferrum_ui::PANE_GAP))
                    })
                    .child(terminal),
            );
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
            .when(!cards, |root| root.bg(colors.terminal_background))
            .text_color(colors.terminal_foreground)
            .when_some(
                self.project.as_ref().filter(|_| compact),
                |root, project| root.child(project.bar.clone()),
            )
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
