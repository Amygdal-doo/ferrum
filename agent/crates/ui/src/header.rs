//! The header: the thread's title (renamed in place) and its ⋯ menu
//! (rename, delete, copy as Markdown); outside a shell host also the
//! thread list toggle, the changes, new-thread and settings buttons.

use ferrum_ui::components::ContextMenu;
use ferrum_ui::prelude::*;
use gpui::{Anchor, Context, Entity, Window};

use crate::actions::*;
use crate::view::ChatView;

impl ChatView {
    /// The ⋯ menu, built when it opens.
    fn thread_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&mut Window, &mut App) -> Option<Entity<ContextMenu>> + 'static {
        let this = cx.entity().downgrade();
        move |window, cx| {
            let this = this.clone();
            Some(ContextMenu::build(window, cx, move |menu, _, _| {
                let run = |f: fn(&mut ChatView, &mut Window, &mut Context<ChatView>)| {
                    let this = this.clone();
                    move |window: &mut Window, cx: &mut App| {
                        this.update(cx, |view, cx| f(view, window, cx)).ok();
                    }
                };
                menu.icon_entry(
                    "Rename",
                    IconName::Pencil,
                    run(|v, window, cx| v.start_rename(window, cx)),
                )
                .icon_entry(
                    "Copy as Markdown",
                    IconName::Copy,
                    run(|v, _, cx| v.copy_markdown(cx)),
                )
                .separator()
                .icon_entry(
                    "Delete",
                    IconName::Trash,
                    run(|v, _, cx| v.delete_current_thread(cx)),
                )
            }))
        }
    }

    pub(crate) fn render_header(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let start = self.header_start.clone().map(|slot| slot(window, cx));
        let end = self.header_end.clone().map(|slot| slot(window, cx));
        let colors = cx.theme().colors().clone();
        let changes = self.changes.len();
        let shell = self.shell;
        let title = match &self.renaming {
            Some((input, _)) => div()
                .flex_1()
                .min_w_0()
                .px_1()
                .rounded(px(6.))
                .border_1()
                .border_color(colors.border_focused)
                .child(input.clone())
                .into_any_element(),
            None => h_flex()
                .id("chat-title")
                .flex_1()
                .min_w_0()
                .gap_1p5()
                .when(shell, |this| {
                    this.child(
                        Icon::new(IconName::GitBranch)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    )
                })
                .child(
                    div().min_w_0().overflow_hidden().child(
                        Label::new(self.thread.title.clone())
                            .size(LabelSize::Small)
                            .single_line()
                            .truncate(),
                    ),
                )
                .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                    if event.click_count() == 2 {
                        this.start_rename(window, cx);
                    }
                }))
                .into_any_element(),
        };
        // In a shell host the row is the window's title row: it drags the
        // window and lines up with the sidebar's.
        let row = if shell {
            ferrum_ui::title_row("chat-header", self.header_lights, window)
        } else {
            h_flex().id("chat-header").h(px(36.))
        };
        let lights = self.header_lights;
        row.flex_none()
            .when(!shell, |this| this.px_1())
            .gap_1()
            .children(start)
            .when(!shell, |this| {
                this.border_b_1()
                    .border_color(colors.border)
                    .bg(colors.toolbar_background)
                    .child(
                        IconButton::new("chat-threads", IconName::ThreadsSidebarLeftOpen)
                            .icon_size(IconSize::Small)
                            .toggle_state(self.show_threads)
                            .tooltip(Tooltip::text("Threads (⌘⌥J)"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_threads(&ToggleThreads, window, cx)
                            })),
                    )
            })
            // `title_row` already leaves room for the traffic lights.
            .when(shell && !lights, |this| this.pl_3())
            .child(title)
            .child(
                PopoverMenu::new("chat-thread-menu")
                    .trigger(ferrum_ui::title_button(
                        "chat-thread-menu-button",
                        IconName::Ellipsis,
                        "Thread",
                    ))
                    .anchor(Anchor::TopRight)
                    .menu(self.thread_menu(cx)),
            )
            .when(changes > 0 && !shell, |this| {
                this.child(
                    Button::new("chat-changes", format!("{changes} changed"))
                        .label_size(LabelSize::XSmall)
                        .style(ButtonStyle::Subtle)
                        .color(Color::Modified)
                        .toggle_state(self.show_changes)
                        .tooltip(Tooltip::text("Review changes (⌘⌥R)"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_changes(&ToggleChanges, window, cx)
                        })),
                )
            })
            .when(!shell, |this| {
                this.child(
                    IconButton::new("chat-new", IconName::Plus)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("New thread (⌘⌥N)"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.new_thread(&NewThread, window, cx)
                        })),
                )
                .child(
                    IconButton::new("chat-settings", IconName::Settings)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Models and API keys"))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_settings(window, cx)),
                        ),
                )
            })
            .children(end)
    }
}
