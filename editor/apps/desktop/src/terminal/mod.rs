//! The integrated terminal panel docked at the bottom of the workspace:
//! terminal tabs (each a [`TerminalView`] over a PTY [`session::Session`]),
//! a tab strip with new / close / hide buttons. Tab bookkeeping is the
//! core crate's [`PanelState`]; sessions keep running while hidden.

mod ime;
mod paint;
mod selection;
pub mod session;
mod view;

use std::path::PathBuf;

use ferrum_ui::prelude::*;
use ferrumeditor_core::terminal::{PanelState, Toggle};
use ui::{IconButton, IconName, IconSize};

use crate::settings::Settings;
pub use view::{TerminalEvent, TerminalView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelEvent {
    /// The panel hid itself (last terminal closed or exited).
    Hidden,
    /// A terminal took focus.
    Focused,
}

pub struct TerminalPanel {
    pub state: PanelState,
    terminals: Vec<Entity<TerminalView>>,
    /// Where new shells start (the project root).
    pub cwd: Option<PathBuf>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PanelEvent> for TerminalPanel {}

impl TerminalPanel {
    pub fn new() -> Self {
        Self {
            state: PanelState::default(),
            terminals: Vec::new(),
            cwd: None,
            _subscriptions: Vec::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.state.open
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.terminals.len()
    }

    /// The active terminal (`terminals` is kept in tab order).
    pub fn active(&self) -> Option<Entity<TerminalView>> {
        self.state
            .active_index()
            .and_then(|i| self.terminals.get(i))
            .cloned()
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.terminals.iter().any(|t| t.read(cx).is_focused(window))
    }

    pub fn focus_active(&self, window: &mut Window, cx: &mut App) {
        if let Some(view) = self.active() {
            let handle = view.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    /// Opens or hides the panel, starting a shell the first time.
    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<bool, String> {
        let outcome = self.state.toggle();
        if outcome == Toggle::OpenedEmpty
            && let Err(error) = self.new_terminal(window, cx)
        {
            self.state.open = false;
            return Err(error);
        }
        cx.notify();
        Ok(self.state.open)
    }

    /// Shows the panel without toggling (and starts a shell if empty).
    pub fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        if !self.state.open {
            self.toggle(window, cx)?;
        }
        Ok(())
    }

    pub fn hide(&mut self, cx: &mut Context<Self>) {
        if self.state.open {
            self.state.toggle();
            cx.notify();
        }
    }

    /// Starts a shell in a new tab (shown and focused).
    pub fn new_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let shell = cx.global::<Settings>().config.terminal_shell.clone();
        let (session, output) = spawn_session(&shell, self.cwd.as_deref())?;
        let id = self.state.add_tab();
        let index = self.state.active_index().unwrap_or(0);
        let view = cx.new(|cx| TerminalView::new(id, session, output, window, cx));
        let subscription = cx.subscribe_in(&view, window, |this, view, event, window, cx| {
            match event {
                TerminalEvent::Exited => {
                    let id = view.read(cx).id;
                    this.close(id, window, cx);
                }
                TerminalEvent::Focused => {
                    let id = view.read(cx).id;
                    if let Some(i) = this.state.tabs().iter().position(|t| *t == id) {
                        this.state.select(i);
                    }
                    cx.emit(PanelEvent::Focused);
                }
                TerminalEvent::Changed => {}
            }
            cx.notify();
        });
        self._subscriptions.push(subscription);
        self.terminals.insert(index, view.clone());
        let handle = view.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        cx.notify();
        Ok(())
    }

    /// Closes terminal `id` (killing its shell).
    pub fn close(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.state.tabs().iter().position(|t| *t == id) else {
            return;
        };
        self.state.close_tab(id);
        self.terminals.remove(index);
        if self.state.open {
            self.focus_active(window, cx);
        } else {
            cx.emit(PanelEvent::Hidden);
        }
        cx.notify();
    }

    pub fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.state.active_id() {
            self.close(id, window, cx);
        }
    }

    pub fn cycle(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        self.state.cycle(delta);
        self.focus_active(window, cx);
        cx.notify();
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.state.select(index);
        self.focus_active(window, cx);
        cx.notify();
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let active = self.state.active_index();
        let tabs = self.terminals.iter().enumerate().map(|(i, view)| {
            let id = view.read(cx).id;
            let is_active = active == Some(i);
            h_flex()
                .id(("terminal-tab", id as usize))
                .group("terminal-tab")
                .h_full()
                .pl_2()
                .pr_1()
                .gap_1()
                .flex_none()
                .border_r_1()
                .border_color(colors.border)
                .when(is_active, |this| this.bg(colors.tab_active_background))
                .child(
                    Icon::new(IconName::Terminal)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(view.read(cx).title())
                        .size(LabelSize::Small)
                        .color(if is_active {
                            Color::Default
                        } else {
                            Color::Muted
                        }),
                )
                .child(
                    IconButton::new(("terminal-tab-close", id as usize), IconName::Close)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Close Terminal"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.close(id, window, cx);
                        })),
                )
                .on_click(cx.listener(move |this, _, window, cx| this.select(i, window, cx)))
        });
        h_flex()
            .flex_none()
            .h(px(28.))
            .w_full()
            .bg(colors.tab_bar_background)
            .border_b_1()
            .border_color(colors.border)
            .child(
                h_flex()
                    .id("terminal-tabs")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_x_scroll()
                    .children(tabs),
            )
            .child(
                h_flex()
                    .flex_none()
                    .px_1()
                    .gap_0p5()
                    .child(
                        IconButton::new("terminal-new", IconName::Plus)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("New Terminal"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Err(error) = this.new_terminal(window, cx) {
                                    eprintln!("ferrumeditor: {error}");
                                }
                            })),
                    )
                    .child(
                        IconButton::new("terminal-hide", IconName::ChevronDown)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Hide Panel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.hide(cx);
                                cx.emit(PanelEvent::Hidden);
                            })),
                    ),
            )
    }
}

impl Render for TerminalPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content: AnyElement = match self.active() {
            Some(view) => view.into_any_element(),
            None => empty_state("No terminal", "Press + to start a shell").into_any_element(),
        };
        v_flex()
            .size_full()
            .child(self.render_tabs(cx))
            .child(div().flex_1().min_h_0().child(content))
    }
}

#[cfg(not(test))]
fn spawn_session(
    shell: &str,
    cwd: Option<&std::path::Path>,
) -> Result<
    (
        session::Session,
        futures::channel::mpsc::UnboundedReceiver<()>,
    ),
    String,
> {
    session::Session::spawn(Some(shell), cwd, 80, 24)
}

/// Tests never start real shells: sessions are detached and record input
/// in [`test_support::SENT`].
#[cfg(test)]
fn spawn_session(
    _shell: &str,
    _cwd: Option<&std::path::Path>,
) -> Result<
    (
        session::Session,
        futures::channel::mpsc::UnboundedReceiver<()>,
    ),
    String,
> {
    let (session, sent) = session::Session::detached(80, 24);
    test_support::SENT.with(|s| s.borrow_mut().push(sent));
    // No process, so no output: the sender is dropped at once.
    let (_, rx) = futures::channel::mpsc::unbounded();
    Ok((session, rx))
}

#[cfg(test)]
pub mod test_support {
    use std::cell::RefCell;
    use std::sync::{Arc, Mutex};

    thread_local! {
        /// Input captured from every detached session, in creation order.
        pub static SENT: RefCell<Vec<Arc<Mutex<Vec<u8>>>>> = const { RefCell::new(Vec::new()) };
    }

    /// Bytes sent to the `index`-th terminal created on this thread.
    pub fn sent(index: usize) -> Vec<u8> {
        SENT.with(|s| {
            s.borrow()
                .get(index)
                .map(|b| b.lock().map(|v| v.clone()).unwrap_or_default())
                .unwrap_or_default()
        })
    }

    pub fn reset() {
        SENT.with(|s| s.borrow_mut().clear());
    }
}
