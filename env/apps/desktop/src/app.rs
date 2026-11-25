//! Root view: the lock screen until the master password is entered, then
//! the vault. Owns the auto-lock timer, the clipboard guard and notices.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ferrum_ui::gpui::{ClipboardItem, KeyBinding, ModifiersChangedEvent};
use ferrum_ui::prelude::*;
use ferrum_ui::search::{
    FindInFiles, FindInView, GoToItem, SearchEverywhere, SearchMode, search_overlay,
};
use ferrumenv_core::crypto::KdfParams;
use ferrumenv_core::store::{self, Session};
use ferrumenv_core::{Result as CoreResult, Vault};

use crate::dialogs::Dialog;
use crate::lock_screen::LockForm;
use crate::secrets::{CLIPBOARD_CLEAR_AFTER, ClipboardGuard};
use crate::sync::SYNC_POLL;
use crate::unlocked::Unlocked;

actions!(ferrum_env, [Lock, NewVariable, OpenSettings, ToggleSidebar]);

/// How often the auto-lock deadline is checked (and the status bar's
/// "Locks in …" refreshed).
const TICK: Duration = Duration::from_secs(15);

pub fn bind_keys(cx: &mut App) {
    // ⌘P (Go to), ⌘⇧F (Find in files), ⌘F (project filter).
    ferrum_ui::search::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("cmd-l", Lock, None),
        KeyBinding::new("ctrl-l", Lock, None),
        KeyBinding::new("cmd-n", NewVariable, None),
        KeyBinding::new("ctrl-n", NewVariable, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("ctrl-b", ToggleSidebar, None),
    ]);
}

/// Where the vault lives and how keys are derived; tests point it at a
/// temp folder with cheap Argon2 parameters.
pub struct EnvConfig {
    pub vault_path: PathBuf,
    /// Used for new vaults and password changes; existing vaults keep the
    /// parameters in their header.
    pub kdf: KdfParams,
    /// Record folder projects in `~/.ferrum/projects.json` (off in tests).
    pub record_recent: bool,
}

impl EnvConfig {
    pub fn from_home() -> Self {
        Self {
            vault_path: store::default_vault_path()
                .unwrap_or_else(|| PathBuf::from(".ferrum/env/vault.json")),
            kdf: KdfParams::DEFAULT,
            record_recent: true,
        }
    }
}

pub(crate) enum Screen {
    /// No vault yet: create the master password.
    Setup,
    Locked,
    Unlocked(Box<Unlocked>),
}

pub struct EnvApp {
    pub(crate) config: EnvConfig,
    pub(crate) screen: Screen,
    pub(crate) form: LockForm,
    pub(crate) notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    pub(crate) clipboard: ClipboardGuard,
    pub(crate) focus_handle: FocusHandle,
    /// The project sidebar of the unlocked vault is shown (⌘B).
    pub(crate) sidebar_open: bool,
    /// Search Everywhere / Go to / Find in files while open.
    pub(crate) search_modal: Option<(Entity<ferrum_ui::search::SearchModal>, Subscription)>,
    double_shift: ferrum_ui::search::DoubleShift,
    _ticker: Task<()>,
    _sync_poller: Task<()>,
}

impl EnvApp {
    pub fn new(config: EnvConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let form = LockForm::new(window, cx);
        let screen = if config.vault_path.exists() {
            Screen::Locked
        } else {
            Screen::Setup
        };
        let ticker = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let alive = this.update_in(cx, |this, window, cx| {
                    this.check_auto_lock(Instant::now(), window, cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        let sync_poller = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(SYNC_POLL).await;
                let alive = this.update_in(cx, |this, window, cx| this.poll_sync(window, cx));
                if alive.is_err() {
                    break;
                }
            }
        });
        let this = Self {
            config,
            screen,
            form,
            notice: None,
            notice_task: None,
            clipboard: ClipboardGuard::default(),
            focus_handle: cx.focus_handle(),
            sidebar_open: true,
            search_modal: None,
            double_shift: Default::default(),
            _ticker: ticker,
            _sync_poller: sync_poller,
        };
        this.form.focus(window, cx);
        this
    }

    pub(crate) fn is_unlocked(&self) -> bool {
        matches!(self.screen, Screen::Unlocked(_))
    }

    pub(crate) fn vault(&self) -> Option<&Unlocked> {
        match &self.screen {
            Screen::Unlocked(u) => Some(u),
            _ => None,
        }
    }

    pub(crate) fn vault_mut(&mut self) -> Option<&mut Unlocked> {
        match &mut self.screen {
            Screen::Unlocked(u) => Some(u),
            _ => None,
        }
    }

    /// Called with the result of creating or unlocking the vault.
    pub(crate) fn finish_unlock(
        &mut self,
        result: CoreResult<(Session, Vault)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.form.busy = false;
        match result {
            Ok((session, vault)) => {
                self.form.error = None;
                let unlocked = Unlocked::new(session, vault, window, cx);
                self.screen = Screen::Unlocked(Box::new(unlocked));
                if let Some(first) = self
                    .vault()
                    .and_then(|u| (!u.vault.projects.is_empty()).then_some(0))
                {
                    self.select_project(first, window, cx);
                }
                window.focus(&self.focus_handle, cx);
            }
            Err(e) => {
                self.form.error = Some(e.to_string().into());
                self.form.focus(window, cx);
            }
        }
        cx.notify();
    }

    /// Drops every decrypted value and the derived key, and shows the lock
    /// screen. Clears the clipboard if it still holds a copied secret.
    pub(crate) fn lock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_unlocked() {
            return;
        }
        self.clipboard.clear_if_ours(cx);
        // The search modal holds copies of the names: close it too.
        self.search_modal = None;
        // Dropping `Unlocked` wipes the session key and the variables.
        self.screen = Screen::Locked;
        self.form.reset(cx);
        self.form.focus(window, cx);
        cx.notify();
    }

    /// Locks when the inactivity deadline has passed at `now`.
    pub(crate) fn check_auto_lock(
        &mut self,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(unlocked) = self.vault() else {
            return;
        };
        if unlocked.auto_lock.is_expired(now) {
            let minutes = unlocked.vault.settings.auto_lock_minutes;
            self.lock(window, cx);
            self.form.error = Some(format!("Locked after {minutes} min of inactivity.").into());
        }
        cx.notify();
    }

    /// Any keyboard or mouse activity in the window.
    fn touch(&mut self) {
        if let Some(unlocked) = self.vault_mut() {
            unlocked.auto_lock.touch(Instant::now());
        }
    }

    /// Copies a secret; the clipboard is cleared after 30 s if unchanged.
    pub(crate) fn copy_secret(&mut self, text: &str, what: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CLIPBOARD_CLEAR_AFTER).await;
            this.update(cx, |this, cx| this.clipboard.clear_if_ours(cx))
                .ok();
        });
        self.clipboard.arm(text, task);
        let seconds = CLIPBOARD_CLEAR_AFTER.as_secs();
        self.show_info(
            format!("{what} copied. The clipboard clears in {seconds} s."),
            cx,
        );
    }

    /// Copies text that isn't secret (e.g. an encrypted bundle).
    pub(crate) fn copy_plain(&mut self, text: String, what: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.show_info(format!("{what} copied."), cx);
    }

    pub(crate) fn show_info(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.show_notice(Notice::info(message), cx);
    }

    pub(crate) fn show_error(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.show_notice(Notice::error(message), cx);
    }

    fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.notice = Some(notice);
        self.notice_task = Some(Notice::dismiss_later(cx, |this: &mut Self, _| {
            this.notice = None;
        }));
        cx.notify();
    }

    fn on_lock(&mut self, _: &Lock, window: &mut Window, cx: &mut Context<Self>) {
        self.lock(window, cx);
    }

    fn on_toggle_switcher(
        &mut self,
        _: &ToggleProjectSwitcher,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(unlocked) = self.vault() {
            let bar = unlocked.bar.clone();
            bar.update(cx, |bar, cx| bar.toggle_switcher(window, cx));
        }
    }

    fn on_cancel(
        &mut self,
        _: &ferrum_ui::menu::Cancel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .vault()
            .is_some_and(|u| matches!(u.dialog, Some(Dialog::SyncConflicts(_))))
        {
            self.snooze_conflicts(window, cx);
        } else if self.vault().is_some_and(|u| u.dialog.is_some()) {
            self.close_dialog(window, cx);
        } else {
            cx.propagate();
        }
    }
}

impl Render for EnvApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let body = match &self.screen {
            Screen::Unlocked(_) => self.render_vault(window, cx),
            Screen::Setup | Screen::Locked => self.render_lock_screen(window, cx),
        };
        v_flex()
            .id("ferrum-env")
            .key_context("FerrumVariables")
            .relative()
            .size_full()
            .text_color(colors.text)
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, _, _, _| {
                this.touch();
                this.double_shift.key_down();
            }))
            .on_modifiers_changed(
                cx.listener(|this, event: &ModifiersChangedEvent, window, cx| {
                    if this.double_shift.modifiers_changed(&event.modifiers) {
                        this.toggle_search(SearchMode::Everywhere, window, cx);
                    }
                }),
            )
            .on_action(cx.listener(|this, _: &SearchEverywhere, window, cx| {
                this.toggle_search(SearchMode::Everywhere, window, cx)
            }))
            .on_action(cx.listener(|this, _: &GoToItem, window, cx| {
                this.toggle_search(SearchMode::GoTo, window, cx)
            }))
            .on_action(cx.listener(|this, _: &FindInFiles, window, cx| {
                this.toggle_search(SearchMode::FindInFiles, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &FindInView, window, cx| {
                    this.focus_project_filter(window, cx)
                }),
            )
            .capture_any_mouse_down(cx.listener(|this, _, _, _| this.touch()))
            .on_mouse_move(cx.listener(|this, _, _, _| this.touch()))
            .on_scroll(cx.listener(|this, _, _, _| this.touch()))
            .on_action(cx.listener(Self::on_lock))
            .on_action(cx.listener(Self::on_toggle_switcher))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(|this, _: &NewVariable, window, cx| {
                this.open_variable_dialog(None, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                this.sidebar_open = !this.sidebar_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.open_settings(window, cx);
            }))
            .child(body)
            .when_some(self.render_dialog(window, cx), |this, dialog| {
                this.child(dialog)
            })
            .when_some(self.notice.as_ref(), |this, notice| {
                this.child(render_notice(
                    notice,
                    cx.listener(|this, _, _, cx| {
                        this.notice = None;
                        cx.notify();
                    }),
                    cx,
                ))
            })
            .when_some(self.search_modal.as_ref(), |this, (modal, _)| {
                this.child(search_overlay(modal))
            })
    }
}
