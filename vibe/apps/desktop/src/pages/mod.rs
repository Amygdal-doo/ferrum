//! The pages that replace the conversation in the main card: Projects,
//! Discussions, Search, Knowledge Center, Sites, Automations, Extensions
//! and Usage. Their inputs live as long as the window; their data is
//! re-read when a page opens.

pub mod automations;
mod extensions;
mod knowledge;
mod projects;
mod search;
mod sites;
mod usage;

use std::path::{Path, PathBuf};

use ferrum_ui::components::StyledTypography;
use ferrum_ui::gpui::{ElementId, Stateful};
use ferrum_ui::prelude::*;
use ferrumagent_ui::core::Mode;
use ferrumagent_ui::core::rules::Rule;
use ferrumagent_ui::core::session::McpServerStatus;

use crate::app::FerrumEngineer;
use crate::automations::Automation;
use crate::extensions::Server;
use crate::nav::Page;
use crate::sites::Sites;
use crate::threads::Hit;

pub struct PageState {
    data_dir: Option<PathBuf>,
    pub search: Entity<TextInput>,
    pub hits: Vec<Hit>,
    pub searching: bool,
    pub(crate) search_task: Option<Task<()>>,
    pub sites: Sites,
    pub site_url: Entity<TextInput>,
    pub site_title: Entity<TextInput>,
    pub automations: Vec<Automation>,
    pub auto_name: Entity<TextInput>,
    pub auto_prompt: Entity<TextInput>,
    pub auto_schedule: Entity<TextInput>,
    pub auto_mode: Mode,
    pub rules: Vec<Rule>,
    pub rule_name: Entity<TextInput>,
    pub servers: Result<Vec<Server>, String>,
    pub mcp_status: Vec<McpServerStatus>,
    pub mcp_name: Entity<TextInput>,
    pub mcp_command: Entity<TextInput>,
    pub(crate) tasks: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

fn input(
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<FerrumEngineer>,
) -> Entity<TextInput> {
    cx.new(|cx| {
        TextInput::new(window, cx)
            .borderless()
            .placeholder(placeholder)
    })
}

impl PageState {
    pub fn new(
        data_dir: Option<&Path>,
        window: &mut Window,
        cx: &mut Context<FerrumEngineer>,
    ) -> Self {
        let search = input("Search threads in every project", window, cx);
        let searching = cx.subscribe_in(&search, window, |this, _, event, _, cx| {
            if matches!(event, TextInputEvent::Changed | TextInputEvent::Submit) {
                this.run_search(cx);
            }
        });
        let auto_prompt = cx.new(|cx| {
            TextInput::new(window, cx)
                .multiline(3)
                .borderless()
                .placeholder("What should the agent do?")
        });
        Self {
            data_dir: data_dir.map(Path::to_path_buf),
            search,
            hits: Vec::new(),
            searching: false,
            search_task: None,
            sites: data_dir
                .map(|d| crate::sites::load(&d.join("sites.json")))
                .unwrap_or_default(),
            site_url: input("https://…", window, cx),
            site_title: input("Title (optional)", window, cx),
            automations: data_dir
                .map(|d| crate::automations::load(&d.join("automations.json")))
                .unwrap_or_default(),
            auto_name: input("Name", window, cx),
            auto_prompt,
            auto_schedule: input("manual · every 4h · daily 09:00", window, cx),
            auto_mode: Mode::Agent,
            rules: Vec::new(),
            rule_name: input("New rule name", window, cx),
            servers: Ok(Vec::new()),
            mcp_status: Vec::new(),
            mcp_name: input("Name", window, cx),
            mcp_command: input("Command (npx -y …) or https:// URL", window, cx),
            tasks: Vec::new(),
            _subscriptions: vec![searching],
        }
    }

    pub(crate) fn sites_file(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join("sites.json"))
    }

    pub(crate) fn automations_file(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join("automations.json"))
    }

    /// Re-reads what `page` shows.
    pub fn refresh(
        &mut self,
        page: Page,
        project: Option<&Path>,
        agent_home: &Path,
        mcp_status: Option<Vec<McpServerStatus>>,
        cx: &mut Context<FerrumEngineer>,
    ) {
        match page {
            Page::Knowledge => {
                self.rules.clear();
                if let Some(root) = project.map(Path::to_path_buf) {
                    let load = cx
                        .background_spawn(async move { ferrumagent_ui::core::rules::load(&root) });
                    self.tasks.push(cx.spawn(async move |this, cx| {
                        let rules = load.await;
                        this.update(cx, |this, cx| {
                            this.pages.rules = rules;
                            cx.notify();
                        })
                        .ok();
                    }));
                }
            }
            Page::Extensions => {
                let file = ferrumagent_ui::core::paths::mcp_config(agent_home);
                self.servers = crate::extensions::list(&file);
                self.mcp_status = mcp_status.unwrap_or_default();
            }
            Page::Sites => {
                if let Some(file) = self.sites_file() {
                    self.sites = crate::sites::load(&file);
                }
            }
            Page::Automations => {
                if let Some(file) = self.automations_file() {
                    self.automations = crate::automations::load(&file);
                }
            }
            _ => {}
        }
        self.tasks.retain(|t| !t.is_ready());
    }

    /// The project changed: drop what belonged to the old one.
    pub fn project_changed(&mut self, _project: &Path, cx: &mut Context<FerrumEngineer>) {
        self.rules.clear();
        cx.notify();
    }
}

impl FerrumEngineer {
    pub(crate) fn render_page(
        &mut self,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match page {
            Page::Chat => div().into_any_element(),
            Page::Projects => self.projects_page(cx),
            Page::Discussions => self.discussions_page(cx),
            Page::Search => self.search_page(cx),
            Page::Knowledge => self.knowledge_page(cx),
            Page::Sites => self.sites_page(cx),
            Page::Automations => self.automations_page(window, cx),
            Page::Extensions => self.extensions_page(cx),
            Page::Usage => self.usage_page(cx),
        }
    }
}

/// A page: a scrolling column with a heading and a one-line summary.
pub(crate) fn frame(page: Page, summary: &str, cx: &App) -> Stateful<Div> {
    let colors = cx.theme().colors();
    v_flex()
        .id(ElementId::Name(format!("page-{}", page.title()).into()))
        .debug_selector(move || format!("page-{}", page.title()))
        .size_full()
        .overflow_y_scroll()
        .px_8()
        .py_6()
        .gap_3()
        .font_ui(cx)
        .text_size(px(13.))
        .child(
            div()
                .text_size(px(20.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(page.title()),
        )
        .child(
            div()
                .text_color(colors.text_muted)
                .pb_2()
                .child(summary.to_string()),
        )
}

/// A list row: rounded, lighter on hover.
pub(crate) fn row(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    let hover = ferrum_ui::hover_fill(cx);
    h_flex()
        .id(id)
        .w_full()
        .min_h(px(40.))
        .px_3()
        .py_1p5()
        .gap_3()
        .rounded(px(8.))
        .hover(move |s| s.bg(hover))
}

/// Title over a muted detail line.
pub(crate) fn two_lines(title: impl Into<SharedString>, detail: impl Into<SharedString>) -> Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .child(div().truncate().child(title.into()))
        .child(
            Label::new(detail.into())
                .size(LabelSize::Small)
                .color(Color::Muted)
                .truncate(),
        )
}

/// A labelled group of rows.
pub(crate) fn section(title: &str, cx: &App) -> Div {
    div()
        .pt_3()
        .text_size(px(12.))
        .text_color(cx.theme().colors().text_muted)
        .child(title.to_string())
}

/// A (borderless) text input in a field box, taking the free width.
pub(crate) fn field(input: &Entity<TextInput>, cx: &App) -> Div {
    let colors = cx.theme().colors();
    div()
        .flex_1()
        .min_w_0()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.editor_background)
        .child(input.clone())
}

pub(crate) fn placeholder(text: &str, cx: &App) -> Div {
    div()
        .px_3()
        .py_2()
        .text_color(cx.theme().colors().text_placeholder)
        .child(text.to_string())
}
