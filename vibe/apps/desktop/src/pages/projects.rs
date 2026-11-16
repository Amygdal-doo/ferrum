//! Projects (recent folders from every Ferrum app; open one or a new
//! folder) and Discussions (the Ask-mode threads of every project).

use chrono::Utc;
use ferrum_ui::components::IconSize;
use ferrum_ui::prelude::*;
use ferrum_ui::project;
use ferrumagent_ui::core::Mode;

use super::{frame, placeholder, row, two_lines};
use crate::app::FerrumEngineer;
use crate::nav::Page;
use crate::threads;

impl FerrumEngineer {
    pub(crate) fn projects_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.reload_recent();
        let home = project::home_dir();
        let mut page = frame(
            Page::Projects,
            "Folders you opened in any Ferrum app. Each has its own chat, threads and layout.",
            cx,
        )
        .child(
            h_flex().child(
                Button::new("projects-open-folder", "Open Folder…")
                    .style(ButtonStyle::Filled)
                    .size(ButtonSize::Compact)
                    .start_icon(Icon::new(IconName::FolderOpen).size(IconSize::Small))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.project_bar
                            .update(cx, |bar, cx| bar.open_folder_dialog(window, cx));
                    })),
            ),
        );
        if self.recent.is_empty() {
            page = page.child(placeholder("No projects yet.", cx));
        }
        for (i, path) in self.recent.clone().into_iter().enumerate() {
            let current = self.project.as_ref() == Some(&path);
            let threads = self
                .index
                .of(&path)
                .iter()
                .filter(|t| t.messages > 0)
                .count();
            let detail = match threads {
                0 => project::display_path(&path, home.as_deref()),
                n => format!(
                    "{} · {n} thread{}",
                    project::display_path(&path, home.as_deref()),
                    if n == 1 { "" } else { "s" }
                ),
            };
            let open = path.clone();
            page = page.child(
                row(("project", i), cx)
                    .cursor_pointer()
                    .child(Icon::new(IconName::Folder).color(Color::Muted))
                    .child(two_lines(project::project_name(&path), detail))
                    .when(current, |r| {
                        r.child(
                            Label::new("Open")
                                .size(LabelSize::Small)
                                .color(Color::Accent),
                        )
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_project(open.clone(), window, cx);
                    })),
            );
        }
        page.into_any_element()
    }

    pub(crate) fn discussions_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut page = frame(
            Page::Discussions,
            "Questions about the code in Ask mode: the agent reads and searches but changes nothing.",
            cx,
        )
        .child(
            h_flex().child(
                Button::new("new-discussion", "New Discussion")
                    .style(ButtonStyle::Filled)
                    .size(ButtonSize::Compact)
                    .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.start_task(Mode::Ask, None, window, cx);
                    })),
            ),
        );
        let list = self.index.with_mode(Mode::Ask);
        if list.is_empty() {
            page = page.child(placeholder("No discussions yet.", cx));
        }
        let now = Utc::now();
        for entry in list {
            let detail = format!(
                "{} · {} · {} messages",
                project::project_name(&entry.project),
                threads::ago(entry.thread.updated, now),
                entry.thread.messages
            );
            let (project, id) = (entry.project.clone(), entry.thread.id.clone());
            page = page.child(
                row(
                    SharedString::from(format!("discussion-{}", entry.thread.id)),
                    cx,
                )
                .cursor_pointer()
                .child(ferrumagent_ui::thread_logo(
                    &entry.thread.agent,
                    px(16.),
                    cx,
                ))
                .child(two_lines(entry.thread.title.clone(), detail))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_thread(&project, &id, window, cx);
                })),
            );
        }
        page.into_any_element()
    }
}
