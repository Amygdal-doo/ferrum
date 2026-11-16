//! The composer's menus, built when they open: the model/agent picker,
//! Agent / Ask / Plan, and the approval levels (how they map onto the
//! permission policy is [`Approval::allows`]). Both pickers work before
//! the session has opened: the choice waits for it.

use ferrum_ui::Notice;
use ferrum_ui::components::ContextMenu;
use ferrum_ui::prelude::*;
use ferrumagent_core::models::ModelStore;
use ferrumagent_core::modes::Mode;
use ferrumagent_core::permissions::Approval;
use ferrumagent_core::session::AgentChoice;
use gpui::{Context, Entity, Window};

use crate::view::ChatView;

impl ChatView {
    /// Providers, models and keys: the session's once open.
    pub(crate) fn models(&self) -> ModelStore {
        self.session
            .as_ref()
            .map_or_else(|| self.models.clone(), |s| s.models())
    }

    /// The agent the composer shows: one chosen before the session opened,
    /// else the thread's.
    pub(crate) fn shown_agent(&self) -> String {
        self.pending_agent
            .as_ref()
            .map_or_else(|| self.thread.agent.clone(), AgentChoice::key)
    }

    /// Hands choices made before the session opened to it.
    pub(crate) fn apply_pending(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        if let Some(choice) = self.pending_agent.take() {
            session.set_agent(choice.clone());
            self.thread.agent = choice.key();
        }
        if let Some(approval) = self.pending_approval.take()
            && let Err(e) = session.set_approval(approval)
        {
            self.show_notice(Notice::error(e), cx);
        }
    }

    /// The model/agent menu, built when it opens.
    pub(crate) fn agent_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&mut Window, &mut App) -> Option<Entity<ContextMenu>> + 'static {
        let this = cx.entity().downgrade();
        let models = self.models();
        let current = self.shown_agent();
        move |window, cx| {
            let options = models.agent_options();
            let this = this.clone();
            let current = current.clone();
            Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                let mut group = String::new();
                for option in options {
                    if option.group != group {
                        if !group.is_empty() {
                            menu = menu.separator();
                        }
                        group = option.group.clone();
                        menu = menu.header(group.clone());
                    }
                    let logo = crate::threads::agent_logo(&option.choice.key());
                    if !option.enabled {
                        menu = menu.disabled_entry(option.label.clone()).with_logo(logo);
                        if let Some(note) = option.note {
                            menu = menu.with_badge(note);
                        }
                        continue;
                    }
                    let this = this.clone();
                    let choice = option.choice.clone();
                    let name = (choice.key(), option.label.clone());
                    menu = menu
                        .toggleable_entry(
                            option.label.clone(),
                            option.choice.key() == current,
                            IconPosition::Start,
                            None,
                            move |_, cx| {
                                let choice = choice.clone();
                                let name = name.clone();
                                this.update(cx, |view, cx| {
                                    view.agent_name = Some(name);
                                    view.set_agent(choice, cx);
                                })
                                .ok();
                            },
                        )
                        .with_logo(logo);
                }
                let test = this.clone();
                menu.separator()
                    .entry("Test this model", None, move |_, cx| {
                        test.update(cx, |view, cx| view.ping_current(cx)).ok();
                    })
                    .entry("Models and API keys…", None, {
                        let this = this.clone();
                        move |window, cx| {
                            this.update(cx, |view, cx| view.open_settings(window, cx))
                                .ok();
                        }
                    })
            }))
        }
    }

    /// Sends the chosen model a tiny request and tells how it went.
    pub(crate) fn ping_current(&mut self, cx: &mut Context<Self>) {
        let Some(AgentChoice::Native { provider, model }) = AgentChoice::parse(&self.shown_agent())
        else {
            self.show_notice(
                Notice::info("External agents sign in on their own: send them a message."),
                cx,
            );
            return;
        };
        let ping = self.models().ping(&provider, &model);
        self.show_notice(Notice::info(format!("Testing {model}…")), cx);
        cx.spawn(async move |this, cx| {
            let result = ping.await;
            this.update(cx, |this, cx| {
                let notice = match result {
                    Ok(pong) => Notice::info(format!("{model}: {}", pong.summary())),
                    Err(e) => Notice::error(format!("{model}: {e}")),
                };
                this.show_notice(notice, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Agent / Ask / Plan, built when it opens.
    pub(crate) fn mode_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&mut Window, &mut App) -> Option<Entity<ContextMenu>> + 'static {
        let this = cx.entity().downgrade();
        let current = self.thread.mode;
        move |window, cx| {
            let this = this.clone();
            Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                menu = menu.header("Mode  ⇧Tab");
                for mode in Mode::ALL {
                    let this = this.clone();
                    menu = menu.toggleable_entry(
                        mode.label(),
                        mode == current,
                        IconPosition::Start,
                        None,
                        move |_, cx| {
                            this.update(cx, |view, cx| view.set_mode(mode, cx)).ok();
                        },
                    );
                }
                menu
            }))
        }
    }

    /// The approval levels, built when it opens.
    pub(crate) fn approval_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&mut Window, &mut App) -> Option<Entity<ContextMenu>> + 'static {
        let this = cx.entity().downgrade();
        let current = self.approval();
        move |window, cx| {
            let this = this.clone();
            Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                menu = menu.header("Approvals");
                for approval in Approval::ALL {
                    let this = this.clone();
                    menu = menu.toggleable_entry(
                        approval.label(),
                        approval == current,
                        IconPosition::Start,
                        None,
                        move |_, cx| {
                            this.update(cx, |view, cx| view.set_approval(approval, cx))
                                .ok();
                        },
                    );
                }
                menu.separator().label(current.description())
            }))
        }
    }

    pub(crate) fn approval(&self) -> Approval {
        if let Some(approval) = self.pending_approval {
            return approval;
        }
        self.session
            .as_ref()
            .map_or_else(|| self.models.approval(), |s| s.approval())
    }

    /// Sets the level now, or (before the session opens) saves it and
    /// hands it to the session when it does.
    pub(crate) fn set_approval(&mut self, approval: Approval, cx: &mut Context<Self>) {
        let saved = match &self.session {
            Some(session) => {
                self.pending_approval = None;
                session.set_approval(approval)
            }
            None => {
                self.pending_approval = Some(approval);
                self.models.set_approval(approval)
            }
        };
        if let Err(e) = saved {
            self.show_notice(Notice::error(e), cx);
        }
        cx.notify();
    }
}

pub(crate) fn mode_icon(mode: Mode) -> IconName {
    match mode {
        Mode::Agent => IconName::Sparkle,
        Mode::Ask => IconName::Info,
        Mode::Plan => IconName::FileTree,
    }
}

pub(crate) fn approval_icon(approval: Approval) -> IconName {
    match approval {
        Approval::Ask => IconName::Lock,
        Approval::AutoEdits => IconName::Pencil,
        Approval::FullAccess => IconName::Zap,
    }
}
