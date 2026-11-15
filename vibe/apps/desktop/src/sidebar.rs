//! The foldable sidebar on the window backdrop: title row (traffic lights,
//! ◧, back / forward), New Task · Projects · Discussion · Search, the
//! project folders with their threads, the threads that need you, finished
//! unseen or are working (across projects), recent conversations, then
//! Knowledge Center · Sites · Automations · Extensions and the footer
//! (usage, settings).

use std::path::{Path, PathBuf};

use chrono::Utc;
use ferrum_ui::components::{IconSize, StyledTypography};
use ferrum_ui::gpui::{Div, ElementId, Stateful};
use ferrum_ui::prelude::*;
use ferrum_ui::project;
use ferrumagent_ui::core::Mode;

use crate::app::{FerrumVibe, SIDEBAR_FOLDERS};
use crate::nav::Page;
use crate::threads::{self, Entry};
use ferrumagent_ui::core::status::{StatusEntry, ThreadStatus, short_ago};

/// Threads shown under an expanded folder.
const FOLDER_THREADS: usize = 8;
/// Rows under "Recent conversations".
const RECENT_THREADS: usize = 8;

fn name(prefix: &str, key: impl std::fmt::Display) -> ElementId {
    ElementId::Name(format!("{prefix}-{key}").into())
}

/// A sidebar row with an icon and a label.
fn nav_row(
    id: impl Into<ElementId>,
    icon: IconName,
    label: impl Into<SharedString>,
    active: bool,
    cx: &App,
) -> Stateful<Div> {
    ferrum_ui::sidebar_item(id, active, cx)
        .child(Icon::new(icon).size(IconSize::Small).color(if active {
            Color::Default
        } else {
            Color::Muted
        }))
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
}

impl FerrumVibe {
    /// ◧ and back / forward: in the sidebar's title row, or at the start
    /// of the main title row while the sidebar is folded.
    pub(crate) fn window_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.ui.sidebar_open;
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                ferrum_ui::sidebar_toggle("toggle-sidebar", open)
                    .tooltip(Tooltip::text(if open {
                        "Hide sidebar (⌘\\)"
                    } else {
                        "Show sidebar (⌘\\)"
                    }))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
            )
            .child(ferrum_ui::settings_button("open-settings"))
            .child(
                ferrum_ui::title_button("go-back", IconName::ArrowLeft, "Back (⌘[)")
                    .disabled(!self.history.can_go_back())
                    .on_click(cx.listener(|this, _, window, cx| this.go_back(window, cx))),
            )
            .child(
                ferrum_ui::title_button("go-forward", IconName::ArrowRight, "Forward (⌘])")
                    .disabled(!self.history.can_go_forward())
                    .on_click(cx.listener(|this, _, window, cx| this.go_forward(window, cx))),
            )
    }

    fn page_row(
        &self,
        page: Page,
        icon: IconName,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        nav_row(
            name("nav", page.title()),
            icon,
            label,
            self.page() == page,
            cx,
        )
        .debug_selector(move || format!("nav-{}", page.title()))
        .on_click(cx.listener(move |this, _, window, cx| this.show_page(page, window, cx)))
    }

    fn thread_row(
        &self,
        project: &Path,
        thread: &ferrumagent_ui::core::store::ThreadSummary,
        active: bool,
        detail: Option<String>,
        indent: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let title = if thread.title.trim().is_empty() {
            "New thread".to_string()
        } else {
            thread.title.clone()
        };
        let (project, id) = (project.to_path_buf(), thread.id.clone());
        let key = format!("{}-{}", if indent { "f" } else { "r" }, thread.id);
        ferrum_ui::sidebar_item(name("thread", key), active, cx)
            .when(indent, |row| row.pl(px(30.)))
            .child(div().flex_1().min_w_0().truncate().child(title))
            .children(detail.map(|d| {
                Label::new(d)
                    .size(LabelSize::XSmall)
                    .color(Color::Placeholder)
                    .truncate()
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_thread(&project, &id, window, cx);
            }))
    }

    /// Threads of every open project's chat that are not Idle, in the
    /// sections Needs you, Done, Working, newest change first.
    fn active_threads(&self, cx: &App) -> Vec<(ThreadStatus, Vec<(Entry, StatusEntry)>)> {
        let mut rows: Vec<(Entry, StatusEntry)> = Vec::new();
        for (project, slot) in &self.chats {
            let chat = slot.view.read(cx);
            for (id, status) in chat.statuses().active() {
                if let Some(thread) = self.index.of(project).iter().find(|t| t.id == id) {
                    let entry = Entry {
                        project: project.clone(),
                        thread: thread.clone(),
                    };
                    rows.push((entry, status.clone()));
                }
            }
        }
        rows.sort_by_key(|(_, s)| std::cmp::Reverse(s.changed));
        ThreadStatus::ORDER
            .into_iter()
            .filter_map(|status| {
                let section: Vec<_> = rows
                    .iter()
                    .filter(|(_, s)| s.status == status)
                    .cloned()
                    .collect();
                (!section.is_empty()).then_some((status, section))
            })
            .collect()
    }

    /// A row of the status sections: logo, title, project, time since the
    /// change and the status mark.
    fn status_row(
        &self,
        entry: &Entry,
        status: &StatusEntry,
        active: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let thread = &entry.thread;
        let title = if thread.title.trim().is_empty() {
            "New thread".to_string()
        } else {
            thread.title.clone()
        };
        let (project, id) = (entry.project.clone(), thread.id.clone());
        ferrum_ui::sidebar_item(name("status-thread", &thread.id), active, cx)
            .child(ferrumagent_ui::thread_logo(&thread.agent, px(14.), cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .py_0p5()
                    .child(div().truncate().child(title))
                    .child(
                        Label::new(project::project_name(&entry.project))
                            .size(LabelSize::XSmall)
                            .color(Color::Placeholder)
                            .truncate(),
                    ),
            )
            .child(
                Label::new(short_ago(status.changed, Utc::now()))
                    .size(LabelSize::XSmall)
                    .color(Color::Placeholder),
            )
            .child(ferrumagent_ui::status_indicator(Some(status), cx))
            .when(!status.summary.is_empty(), |row| {
                row.tooltip(Tooltip::text(status.summary.clone()))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_thread(&project, &id, window, cx);
            }))
    }

    fn folder(&self, path: &PathBuf, current_thread: Option<&str>, cx: &mut Context<Self>) -> Div {
        let current = self.project.as_ref() == Some(path);
        let expanded = self.ui.is_expanded(path, current);
        let toggle_path = path.clone();
        let open_path = path.clone();
        let header = ferrum_ui::sidebar_item(name("folder", path.display()), false, cx)
            .child(
                div()
                    .id(name("folder-chevron", path.display()))
                    .flex_none()
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.ui.toggle_folder(&toggle_path);
                        this.save_ui();
                        cx.notify();
                    })),
            )
            .child(
                Icon::new(if current {
                    IconName::FolderOpen
                } else {
                    IconName::Folder
                })
                .size(IconSize::Small)
                .color(Color::Muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(current, |d| d.text_color(cx.theme().colors().text))
                    .child(project::project_name(path)),
            )
            .tooltip(Tooltip::text(path.display().to_string()))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_project(open_path.clone(), window, cx);
            }));
        let mut col = v_flex().gap_0p5().child(header);
        if expanded {
            let list = self.index.of(path);
            if list.is_empty() {
                col = col.child(
                    div()
                        .pl(px(36.))
                        .py_0p5()
                        .text_size(px(12.))
                        .text_color(cx.theme().colors().text_placeholder)
                        .child("No threads yet"),
                );
            }
            for thread in list.iter().filter(|t| t.messages > 0).take(FOLDER_THREADS) {
                let active = current
                    && self.page() == Page::Chat
                    && current_thread == Some(thread.id.as_str());
                col = col.child(self.thread_row(path, thread, active, None, true, cx));
            }
        }
        col
    }

    pub(crate) fn sidebar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let current_thread = self.current_thread_id(cx);
        let mut scroll = v_flex()
            .id("sidebar-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .gap_0p5()
            .pb_2()
            .child(
                h_flex()
                    .pr_2()
                    .child(
                        div()
                            .flex_1()
                            .child(ferrum_ui::sidebar_label("Folders", cx)),
                    )
                    .child(
                        ferrum_ui::title_button("open-folder", IconName::Plus, "Open Folder…")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.project_bar
                                    .update(cx, |bar, cx| bar.open_folder_dialog(window, cx));
                            })),
                    ),
            );
        for path in self.recent.iter().take(SIDEBAR_FOLDERS + 1) {
            scroll = scroll.child(self.folder(path, current_thread.as_deref(), cx));
        }
        let active = self.active_threads(cx);
        for (status, rows) in &active {
            scroll = scroll.child(ferrum_ui::sidebar_label(
                format!("{} · {}", status.label(), rows.len()),
                cx,
            ));
            for (entry, entry_status) in rows {
                let shown = self.project.as_ref() == Some(&entry.project)
                    && self.page() == Page::Chat
                    && current_thread.as_deref() == Some(entry.thread.id.as_str());
                scroll = scroll.child(self.status_row(entry, entry_status, shown, cx));
            }
        }
        let mut recent = self.index.recent(RECENT_THREADS + active.len());
        recent.retain(|e| {
            !active
                .iter()
                .any(|(_, rows)| rows.iter().any(|(a, _)| a.thread.id == e.thread.id))
        });
        recent.truncate(RECENT_THREADS);
        if !recent.is_empty() {
            scroll = scroll.child(ferrum_ui::sidebar_label("Recent conversations", cx));
            let now = Utc::now();
            for entry in &recent {
                let active = self.project.as_ref() == Some(&entry.project)
                    && self.page() == Page::Chat
                    && current_thread.as_deref() == Some(entry.thread.id.as_str());
                let detail = threads::ago(entry.thread.updated, now);
                scroll = scroll.child(self.thread_row(
                    &entry.project,
                    &entry.thread,
                    active,
                    Some(detail),
                    false,
                    cx,
                ));
            }
        }
        let new_task = nav_row("new-task", IconName::Plus, "New Task", false, cx).on_click(
            cx.listener(|this, _, window, cx| {
                this.start_task(Mode::Agent, None, window, cx);
            }),
        );
        v_flex()
            .flex_none()
            .w(px(self.ui.sidebar_width))
            .h_full()
            .font_ui(cx)
            .text_size(px(13.))
            .text_color(colors.text)
            .child(
                ferrum_ui::title_row("sidebar-title", true, window)
                    .child(self.window_controls(cx))
                    .child(div().flex_1()),
            )
            .child(
                v_flex()
                    .gap_0p5()
                    .pt_1()
                    .child(new_task)
                    .child(self.page_row(Page::Projects, IconName::Blocks, "Projects", cx))
                    .child(self.page_row(Page::Discussions, IconName::Pencil, "Discussion", cx))
                    .child(self.page_row(Page::Search, IconName::MagnifyingGlass, "Search", cx)),
            )
            .child(scroll)
            .child(div().flex_none().mx_3().h(px(1.)).bg(colors.border_variant))
            .child(
                v_flex()
                    .flex_none()
                    .gap_0p5()
                    .py_1()
                    .child(self.page_row(
                        Page::Knowledge,
                        IconName::Bookmark,
                        "Knowledge Center",
                        cx,
                    ))
                    .child(self.page_row(Page::Sites, IconName::Public, "Sites", cx))
                    .child(self.page_row(Page::Automations, IconName::RotateCw, "Automations", cx))
                    .child(self.page_row(Page::Extensions, IconName::Box, "Extensions", cx)),
            )
            .child(self.footer(cx))
    }

    fn footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let usage_active = self.page() == Page::Usage;
        h_flex()
            .flex_none()
            .h(px(40.))
            .pl_3p5()
            .pr_2()
            .gap_0p5()
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("FerrumVibe"),
            )
            .child(
                ferrum_ui::title_button("usage", IconName::Zap, "Usage")
                    .toggle_state(usage_active)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_page(Page::Usage, window, cx);
                    })),
            )
            .child(
                PopoverMenu::new("settings-menu")
                    .anchor(gpui::Anchor::BottomLeft)
                    .trigger(ferrum_ui::title_button(
                        "settings",
                        IconName::Settings,
                        "Settings",
                    ))
                    .menu(move |window, cx| {
                        let weak = weak.clone();
                        Some(ContextMenu::build(window, cx, move |menu, _, _| {
                            let chat = weak.clone();
                            menu.icon_entry(
                                "Chat settings",
                                IconName::Sparkle,
                                move |window, cx| {
                                    chat.update(cx, |this, cx| this.chat_settings(window, cx))
                                        .ok();
                                },
                            )
                            .icon_entry(
                                "FerrumVibe settings…",
                                IconName::Settings,
                                |_, cx| {
                                    ferrum_ui::open_settings(None, cx);
                                },
                            )
                        }))
                    }),
            )
    }
}
