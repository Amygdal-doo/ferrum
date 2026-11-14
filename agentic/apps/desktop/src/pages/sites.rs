//! Sites: web pages saved for this project; "Use in chat" adds one to the
//! chat's context.

use ferrum_ui::components::IconSize;
use ferrum_ui::prelude::*;
use ferrumagent_ui::core::context::Mention;

use super::{field, frame, placeholder, row, two_lines};
use crate::app::FerrumAgentic;
use crate::nav::Page;

impl FerrumAgentic {
    fn save_sites(&mut self, cx: &mut Context<Self>) {
        if let Some(file) = self.pages.sites_file()
            && let Err(e) = crate::sites::save(&file, &self.pages.sites)
        {
            self.show_notice(Notice::error(format!("Could not save sites: {e}")), cx);
        }
        cx.notify();
    }

    fn add_site(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let url = self.pages.site_url.read(cx).text().to_string();
        let title = self.pages.site_title.read(cx).text().to_string();
        if self.pages.sites.add(&project, &url, &title).is_none() {
            self.show_notice(Notice::error("That doesn't look like a web address."), cx);
            return;
        }
        for input in [self.pages.site_url.clone(), self.pages.site_title.clone()] {
            input.update(cx, |input, cx| input.set_text("", cx));
        }
        self.save_sites(cx);
    }

    /// Adds `url` to the chat's context and shows the chat.
    pub fn use_site(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(chat) = self.chat() {
            chat.update(cx, |chat, cx| chat.add_mention(Mention::Url { url }, cx));
            self.show_page(Page::Chat, window, cx);
        }
    }

    pub(crate) fn sites_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut page = frame(
            Page::Sites,
            "Docs, dashboards and specs for this project. Use one in the chat to give the agent the page.",
            cx,
        );
        let Some(project) = self.project.clone() else {
            return page
                .child(placeholder("Open a project to save sites for it.", cx))
                .into_any_element();
        };
        page = page.child(
            h_flex()
                .gap_2()
                .child(field(&self.pages.site_url, cx))
                .child(field(&self.pages.site_title, cx).max_w(px(240.)))
                .child(
                    Button::new("add-site", "Add")
                        .style(ButtonStyle::Filled)
                        .size(ButtonSize::Compact)
                        .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
                        .on_click(cx.listener(|this, _, _, cx| this.add_site(cx))),
                ),
        );
        let sites = self.pages.sites.of(&project).to_vec();
        if sites.is_empty() {
            page = page.child(placeholder("No sites saved for this project.", cx));
        }
        for (i, site) in sites.into_iter().enumerate() {
            let (use_url, remove_url) = (site.url.clone(), site.url.clone());
            let remove_project = project.clone();
            page = page.child(
                row(("site", i), cx)
                    .child(Icon::new(IconName::Public).color(Color::Muted))
                    .child(two_lines(site.title, site.url))
                    .child(
                        Button::new(("use-site", i), "Use in chat")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.use_site(use_url.clone(), window, cx);
                            })),
                    )
                    .child(
                        IconButton::new(("remove-site", i), IconName::Trash)
                            .icon_size(IconSize::Small)
                            .icon_color(Color::Muted)
                            .tooltip(Tooltip::text("Remove"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.pages.sites.remove(&remove_project, &remove_url);
                                this.save_sites(cx);
                            })),
                    ),
            );
        }
        page.into_any_element()
    }
}
