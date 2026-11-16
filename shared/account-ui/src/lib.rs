//! The Ferrum account in every desktop app, on top of `ferrum-account`
//! (sign-in, tokens, `account.json`) and `ferrum-ui` (widgets, Settings):
//!
//! - [`init`] at start: shows the cached user right away, then checks the
//!   session in the background; an app that follows the launcher and has
//!   no session yet signs in from the launcher's (single sign-on). Adds
//!   Settings › Account to the app's Settings window.
//! - [`AccountButton`] / [`account_button`] for the right end of the title
//!   row: **Sign in**, or the avatar and its menu.
//! - [`model::Status`]: which menu items and sign-in choices show, by mode.
//!
//! Kept out of `ferrum-ui` so the widget crate stays free of networking,
//! the keychain and Tokio; `ferrum-account` stays free of GPUI.
//!
//! ```ignore
//! ferrum_ui::run(config, |window, cx| {
//!     ferrum_account_ui::init(ferrum_account::App::Git, cx);
//!     …
//! });
//! // in the title row:
//! row.children(ferrum_account_ui::account_button(cx))
//! ```

mod avatar;
mod button;
pub mod model;
mod page;
mod runtime;
mod state;

use std::rc::Rc;

use ferrum_ui::gpui::{App, AppContext as _, Entity, Global, SharedString};

pub use avatar::avatar;
pub use button::{AccountButton, account_button};
pub use ferrum_account::App as AccountApp;
pub use page::AccountPage;
pub use state::{AccountState, MessageHandler};

struct GlobalAccount(Entity<AccountState>);

impl Global for GlobalAccount {}

/// Sets up the account for `app` (once; later calls do nothing): loads the
/// cached user, checks the session in the background and adds Settings ›
/// Account. Call from the `ferrum_ui::run` build closure.
pub fn init(app: AccountApp, cx: &mut App) {
    if cx.has_global::<GlobalAccount>() {
        return;
    }
    let state = cx.new(|cx| {
        let mut state = AccountState::new(app);
        state.start(cx);
        state
    });
    cx.set_global(GlobalAccount(state));
    ferrum_ui::settings::add_shared_page(Rc::new(AccountPage), cx);
}

/// The account state, once [`init`] ran.
pub fn state(cx: &App) -> Option<Entity<AccountState>> {
    cx.try_global::<GlobalAccount>().map(|g| g.0.clone())
}

/// Shows account messages ("Signed in as @ada", failures) with `handler`
/// instead of an in-app toast (FerrumLauncher's notice line).
pub fn set_message_handler(cx: &mut App, handler: impl Fn(SharedString, bool, &mut App) + 'static) {
    if let Some(state) = state(cx) {
        state.update(cx, |state, _| state.set_message_handler(Rc::new(handler)));
    }
}
