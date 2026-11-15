//! Usage: tokens the agents used, this week and in total, per project and
//! for the biggest threads.

use chrono::Utc;
use ferrum_ui::components::StyledTypography;
use ferrum_ui::prelude::*;
use ferrum_ui::project;

use super::{frame, placeholder, row, section, two_lines};
use crate::app::FerrumEngineer;
use crate::nav::Page;
use crate::threads::format_tokens;

fn stat(label: &str, value: String, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    v_flex()
        .min_w(px(160.))
        .px_4()
        .py_3()
        .gap_1()
        .rounded(px(10.))
        .border_1()
        .border_color(colors.border_variant)
        .child(
            div()
                .text_size(px(12.))
                .text_color(colors.text_muted)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(22.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(value),
        )
}

impl FerrumEngineer {
    pub(crate) fn usage_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let usage = self.index.usage(Utc::now());
        let mut page = frame(
            Page::Usage,
            "Tokens used by the agents in your recent projects (input and output).",
            cx,
        )
        .child(
            h_flex()
                .gap_3()
                .font_ui(cx)
                .child(stat("This week", format_tokens(usage.week), cx))
                .child(stat("Total", format_tokens(usage.total), cx)),
        );
        page = page.child(section("By project", cx));
        if usage.projects.is_empty() {
            page = page.child(placeholder("No usage recorded yet.", cx));
        }
        for (i, p) in usage.projects.iter().enumerate() {
            page = page.child(
                row(("usage-project", i), cx)
                    .child(Icon::new(IconName::Folder).color(Color::Muted))
                    .child(two_lines(
                        project::project_name(&p.project),
                        format!("{} this week", format_tokens(p.week)),
                    ))
                    .child(Label::new(format_tokens(p.total)).size(LabelSize::Small)),
            );
        }
        if !usage.threads.is_empty() {
            page = page.child(section("Biggest threads", cx));
        }
        for (i, entry) in usage.threads.into_iter().enumerate() {
            let tokens = format_tokens(crate::threads::tokens(&entry.thread));
            let (project, id) = (entry.project.clone(), entry.thread.id.clone());
            page = page.child(
                row(("usage-thread", i), cx)
                    .cursor_pointer()
                    .child(two_lines(
                        entry.thread.title.clone(),
                        project::project_name(&entry.project),
                    ))
                    .child(Label::new(tokens).size(LabelSize::Small))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_thread(&project, &id, window, cx);
                    })),
            );
        }
        page.into_any_element()
    }
}
