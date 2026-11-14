//! The window: the native title bar is hidden; the sidebar sits on the
//! backdrop, and beside it a title row (window controls while the sidebar
//! is folded; the tasks list and the Git / Term toggles on the right) over
//! the cards: the chat or a page, then the Git and Term panes.

use chrono::Utc;
use ferrum_ui::components::{Clickable, Toggleable};
use ferrum_ui::gpui::{Axis, DragMoveEvent, MouseButton};
use ferrum_ui::prelude::*;

use crate::app::{
    FerrumAgentic, GoBack, GoForward, KEY_CONTEXT, NewDiscussion, NewTask, ShowSearch, ToggleGit,
    ToggleSidebar, ToggleTerm,
};
use crate::layout::Column;
use crate::nav::{MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH, Page};
use crate::threads;

/// Drag id of the splitter between the sidebar and the cards.
const SIDEBAR_SPLITTER: &str = "agentic-sidebar";

impl FerrumAgentic {
    /// ☰: this project's threads with their mode and age; a click opens one.
    fn tasks_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let project = self.project.clone();
        let list: Vec<(String, String)> = match &project {
            Some(p) => {
                let now = Utc::now();
                self.index
                    .of(p)
                    .iter()
                    .filter(|t| t.messages > 0)
                    .take(30)
                    .map(|t| {
                        let title = if t.title.trim().is_empty() {
                            "New thread"
                        } else {
                            t.title.as_str()
                        };
                        let label = format!(
                            "{title}  ·  {} · {}",
                            t.mode.label(),
                            threads::ago(t.updated, now)
                        );
                        (t.id.clone(), label)
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        PopoverMenu::new("tasks-menu")
            .anchor(gpui::Anchor::TopRight)
            .trigger(ferrum_ui::title_button(
                "tasks",
                IconName::CircleCheck,
                "This project's tasks",
            ))
            .menu(move |window, cx| {
                let (weak, project, list) = (weak.clone(), project.clone(), list.clone());
                Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                    menu = menu.header("Tasks");
                    if list.is_empty() {
                        return menu.disabled_entry("No tasks in this project yet");
                    }
                    for (id, label) in list {
                        let (weak, project) = (weak.clone(), project.clone());
                        menu = menu.entry(label, None, move |window, cx| {
                            if let Some(project) = &project {
                                weak.update(cx, |this, cx| {
                                    this.open_thread(project, &id, window, cx)
                                })
                                .ok();
                            }
                        });
                    }
                    menu
                }))
            })
    }

    fn pane_toggle(
        &self,
        column: Column,
        icon: IconName,
        key: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let visible = self.layout.is_visible(column);
        let verb = if visible { "Hide" } else { "Show" };
        ferrum_ui::title_button(
            crate::columns::id("toggle", column),
            icon,
            format!("{verb} {} ({key})", column.title()),
        )
        .toggle_state(visible)
        .when(visible, |b| b.icon_color(Color::Default))
        .on_click(cx.listener(move |this, _, window, cx| this.toggle(column, window, cx)))
    }

    /// The tasks list and the Git / Terminal toggles, at the right of the
    /// chat's (or a page's) header row.
    pub(crate) fn header_end(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(self.tasks_menu(cx))
            .child(self.pane_toggle(Column::Git, IconName::GitBranch, "⌥⌘2", cx))
            .child(self.pane_toggle(Column::Term, IconName::Terminal, "⌥⌘3", cx))
    }

    /// A page's header row (the page replaces the chat in its card).
    pub(crate) fn page_header(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let folded = !self.ui.sidebar_open;
        ferrum_ui::title_row("page-header", folded, window)
            .gap_1()
            .when(!folded, |row| row.pl_3())
            .when(folded, |row| row.child(self.window_controls(cx)))
            .child(
                Label::new(self.page().title())
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::SEMIBOLD),
            )
            .child(div().flex_1())
            .child(self.header_end(cx))
    }

    /// Hands our controls to the chat's header row (once per chat and
    /// sidebar state: setting them re-renders the chat).
    fn sync_chat_header(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = self.chat() else { return };
        let folded = !self.ui.sidebar_open;
        let key = (chat.entity_id(), folded);
        if self.header_slots_for == Some(key) {
            return;
        }
        self.header_slots_for = Some(key);
        let weak = cx.weak_entity();
        let start = folded.then(|| {
            let weak = weak.clone();
            std::rc::Rc::new(move |_: &mut Window, cx: &mut App| {
                weak.update(cx, |this, cx| this.window_controls(cx).into_any_element())
                    .unwrap_or_else(|_| div().into_any_element())
            }) as ferrumagent_ui::HeaderSlot
        });
        let end = std::rc::Rc::new(move |_: &mut Window, cx: &mut App| {
            weak.update(cx, |this, cx| this.header_end(cx).into_any_element())
                .unwrap_or_else(|_| div().into_any_element())
        }) as ferrumagent_ui::HeaderSlot;
        chat.update(cx, |chat, cx| {
            chat.set_header_slots(folded, start, Some(end), cx)
        });
    }

    /// Hidden: only the ⌥⌘O switcher the project bar opens is drawn.
    fn hidden_project_bar(&self) -> impl IntoElement {
        div().absolute().top_0().left_0().w(px(0.)).h(px(0.)).child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w(px(320.))
                .invisible()
                .child(self.project_bar.clone()),
        )
    }

    fn resize_sidebar(
        &mut self,
        event: &DragMoveEvent<DraggedSplitter>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.drag(cx).id.as_ref() != SIDEBAR_SPLITTER {
            return;
        }
        let x = f32::from(event.event.position.x - event.bounds.left());
        self.ui.sidebar_width = x.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        self.resizing_sidebar = true;
        cx.notify();
    }
}

impl Render for FerrumAgentic {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.follow_focus(window, cx);
        let text = cx.theme().colors().text;
        let notice = self.notice.as_ref().map(|notice| {
            render_notice(
                notice,
                cx.listener(|this, _, _, cx| {
                    this.notice = None;
                    cx.notify();
                }),
                cx,
            )
        });
        let sidebar_open = self.ui.sidebar_open;
        self.sync_chat_header(cx);
        // The cards start at the top, like FerrumTerm's: the chat's own
        // header row (or a page's) holds the window controls and toggles.
        let main = v_flex()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .pt(ferrum_ui::PANE_GAP)
            .child(self.hidden_project_bar())
            .child(self.columns(window, cx));
        h_flex()
            .id("ferrumagentic")
            .key_context(KEY_CONTEXT)
            .track_focus(self.focus_handle())
            .on_action(cx.listener(|this, _: &ToggleGit, window, cx| {
                this.toggle(Column::Git, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleTerm, window, cx| {
                this.toggle(Column::Term, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| this.toggle_sidebar(cx)))
            .on_action(cx.listener(|this, _: &NewTask, window, cx| {
                this.start_task(ferrumagent_ui::core::Mode::Agent, None, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewDiscussion, window, cx| {
                this.start_task(ferrumagent_ui::core::Mode::Ask, None, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GoBack, window, cx| this.go_back(window, cx)))
            .on_action(cx.listener(|this, _: &GoForward, window, cx| this.go_forward(window, cx)))
            .on_action(cx.listener(|this, _: &ShowSearch, window, cx| {
                this.show_page(Page::Search, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleProjectSwitcher, window, cx| {
                this.project_bar
                    .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
            }))
            .on_drag_move(cx.listener(Self::resize_sidebar))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    if std::mem::take(&mut this.resizing_sidebar) {
                        this.save_ui();
                    }
                }),
            )
            .relative()
            .size_full()
            .text_color(text)
            .when(sidebar_open, |root| {
                root.child(self.sidebar(window, cx))
                    .child(ferrum_ui::gap_splitter(
                        SIDEBAR_SPLITTER,
                        Axis::Horizontal,
                        cx,
                    ))
            })
            .child(main)
            .children(notice)
    }
}
