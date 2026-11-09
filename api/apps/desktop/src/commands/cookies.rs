//! Cookie jar: load for the Cookies dialog, delete and clear.

use crate::app::FerrumApi;
use crate::state::Dialog;
use ferrumapi_application::StoreError;
use ferrumapi_domain::CookieJar;
use gpui::Context;
use std::future::Future;

impl FerrumApi {
    /// Loads the jar and opens the Cookies dialog.
    pub fn open_cookies(&mut self, cx: &mut Context<Self>) {
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.cookie_jar().await },
            |this, result, cx| match result {
                Ok(jar) => this.state.dialog = Some(Dialog::Cookies(Box::new(jar))),
                Err(e) => this.notify(format!("Could not load cookies: {e}"), true, cx),
            },
        );
    }

    pub fn remove_cookie(
        &mut self,
        domain: String,
        path: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let svc = self.services.clone();
        self.update_jar(
            async move { svc.delete_cookie(domain, path, name).await },
            cx,
        );
    }

    /// Clears every cookie, or only those of `domain`.
    pub fn clear_cookies(&mut self, domain: Option<String>, cx: &mut Context<Self>) {
        let svc = self.services.clone();
        self.update_jar(async move { svc.clear_cookies(domain).await }, cx);
    }

    /// Runs a jar change and shows the resulting jar if the dialog is open.
    fn update_jar(
        &mut self,
        change: impl Future<Output = Result<CookieJar, StoreError>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.io(cx, change, |this, result, cx| match result {
            Ok(jar) => {
                if let Some(Dialog::Cookies(shown)) = this.state.dialog.as_mut() {
                    **shown = jar;
                }
            }
            Err(e) => this.notify(format!("Could not update cookies: {e}"), true, cx),
        });
    }
}
