//! OAuth 2 tokens (sign-in, forget) and keychain-backed auth secrets.

use super::background;
use crate::app::FerrumApi;
use ferrumapi_application::{CancellationToken, OAuthOpener, Prepared};
use ferrumapi_domain::{Auth, HttpRequest, TabId, VariableSet};
use gpui::Context;

impl FerrumApi {
    /// "Get new access token": runs the grant (the browser sign-in for
    /// authorization code) and caches the token for the active environment.
    pub fn oauth2_get_token(&mut self, auth: Auth, variables: VariableSet, cx: &mut Context<Self>) {
        let Auth::OAuth2(cfg) = &auth else {
            return;
        };
        let secret_ref = cfg.secret_ref.clone();
        let prepared = Prepared {
            request: HttpRequest {
                auth,
                ..HttpRequest::default()
            },
            variables,
            environment: self.state.active_env,
        };
        // The services runtime hands the sign-in URL to the UI thread,
        // which opens the default browser.
        let (url_tx, url_rx) = tokio::sync::oneshot::channel::<String>();
        let open: OAuthOpener = Box::new(move |url| {
            let _ = url_tx.send(url);
        });
        cx.spawn(async move |this, cx| {
            if let Ok(url) = url_rx.await {
                let _ = this.update(cx, |_, cx| cx.open_url(&url));
            }
        })
        .detach();
        self.state
            .oauth_status
            .insert(secret_ref.clone(), "Getting a token…".to_owned());
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.oauth2_authorize(prepared, open, CancellationToken::new())
                    .await
            })
            .await;
            let status = match result {
                Ok(Ok(token)) => match token.expires_at {
                    Some(at) => {
                        let mins = (at - chrono::Utc::now().timestamp()).max(0) / 60;
                        format!("Token cached for this environment (expires in {mins} min).")
                    }
                    None => "Token cached for this environment.".to_owned(),
                },
                Ok(Err(e)) => format!("Could not get a token: {e}"),
                Err(e) => format!("Could not get a token: {e}"),
            };
            let _ = this.update(cx, |this, cx| {
                this.state.oauth_status.insert(secret_ref, status);
                cx.notify();
            });
        })
        .detach();
    }

    /// Forgets the token cached for the active environment.
    pub fn oauth2_forget_token(&mut self, secret_ref: String, cx: &mut Context<Self>) {
        let env = self.state.active_env;
        let svc = self.services.clone();
        self.state.oauth_status.insert(
            secret_ref.clone(),
            "Token forgotten for this environment.".to_owned(),
        );
        self.io(
            cx,
            async move {
                svc.oauth2_clear(&secret_ref, env).await;
                Ok::<(), String>(())
            },
            Self::report("Could not forget the token"),
        );
    }

    /// Loads a tab's keychain secrets when its auth arrived without them
    /// (reopened from history, a project file…). Tried once per account.
    pub fn hydrate_tab_auth(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some(mut auth) = self.state.tab(tab_id).map(|t| t.request.auth.clone()) else {
            return;
        };
        let Some(account) = auth.secret_ref().map(str::to_owned) else {
            return;
        };
        if !auth.secrets_empty() || !self.state.hydrated.insert(account.clone()) {
            return;
        }
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let loaded = background(&services, async move {
                svc.hydrate_secrets(vec![&mut auth]).await;
                auth
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                let Ok(loaded) = loaded else { return };
                if let Some(tab) = this.state.tabs.iter_mut().find(|t| t.id == tab_id)
                    && tab.request.auth.secret_ref() == Some(account.as_str())
                    && tab.request.auth.secrets_empty()
                {
                    tab.request.auth = loaded;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
