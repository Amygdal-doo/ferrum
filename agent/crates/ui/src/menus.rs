//! The composer's menus, built when they open: the model/agent picker,
//! Agent / Ask / Plan, and the approval levels (how they map onto the
//! permission policy is [`Approval::allows`]).

use ferrum_ui::components::ContextMenu;
use ferrum_ui::prelude::*;
use ferrumagent_core::modes::Mode;
use ferrumagent_core::permissions::Approval;
use gpui::{Context, Entity, Window};

use crate::view::ChatView;

impl ChatView {
    /// The model/agent menu, built when it opens.
    pub(crate) fn agent_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&mut Window, &mut App) -> Option<Entity<ContextMenu>> + 'static {
        let this = cx.entity().downgrade();
        let session = self.session.clone();
        let current = self.thread.agent.clone();
        move |window, cx| {
            let options = session
                .as_ref()
                .map(|s| s.agent_options())
                .unwrap_or_default();
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
                    let this = this.clone();
                    let choice = option.choice.clone();
                    menu = menu
                        .toggleable_entry(
                            option.label.clone(),
                            option.choice.key() == current,
                            IconPosition::Start,
                            None,
                            move |_, cx| {
                                let choice = choice.clone();
                                this.update(cx, |view, cx| view.set_agent(choice, cx)).ok();
                            },
                        )
                        .with_logo(crate::threads::agent_logo(&option.choice.key()));
                }
                menu.separator().entry("Models and API keys…", None, {
                    let this = this.clone();
                    move |window, cx| {
                        this.update(cx, |view, cx| view.open_settings(window, cx))
                            .ok();
                    }
                })
            }))
        }
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
        self.session
            .as_ref()
            .map(|s| s.approval())
            .unwrap_or_default()
    }

    pub(crate) fn set_approval(&mut self, approval: Approval, cx: &mut Context<Self>) {
        if let Some(session) = &self.session
            && let Err(e) = session.set_approval(approval)
        {
            self.show_notice(ferrum_ui::Notice::error(e), cx);
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
