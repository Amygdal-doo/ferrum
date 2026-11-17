//! Registry sign-in: the engine checks the credentials (`POST /auth`), then
//! they go to the CLI's credential helper or the OS keychain (see
//! `ferrumdock_engine::credentials`). Pulls look them up on their own.

use crate::app::FerrumContainers;
use crate::state::{Dialog, RegistryPreset};
use ferrumdock_engine::credentials::Credentials;
use ferrumdock_engine::ops;
use ferrumdock_engine::registry::{self, RegistryAuth};
use gpui::Context;
use std::sync::Arc;

/// Runs a blocking credential call off the async runtime's workers.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

impl FerrumContainers {
    pub fn open_registries(&mut self, cx: &mut Context<Self>) {
        self.state.dialog = Some(Dialog::Registries);
        self.state.registries.error = None;
        self.load_registries(cx);
        cx.notify();
    }

    /// Lists the registries with stored credentials.
    pub fn load_registries(&mut self, cx: &mut Context<Self>) {
        self.spawn_run(
            cx,
            blocking(|| Ok(Credentials::detect().list())),
            |app, result, _| app.state.registries.signed_in = Some(result),
        );
    }

    /// Fills the form for `preset` (server and username; the secret is cleared).
    pub fn pick_registry_preset(&mut self, preset: RegistryPreset, cx: &mut Context<Self>) {
        self.state.registries.preset = preset;
        self.state.registries.error = None;
        let (server, username) = preset.defaults();
        let inputs = &self.inputs.registry;
        inputs.server.update(cx, |input, cx| {
            input.set_text(server, cx);
            input.set_placeholder(preset.server_placeholder(), cx);
        });
        inputs
            .username
            .update(cx, |input, cx| input.set_text(username, cx));
        inputs.secret.update(cx, |input, cx| input.set_text("", cx));
        cx.notify();
    }

    /// `docker login`: checked by the engine, then stored. The secret field
    /// is cleared whatever the outcome; the secret is never logged.
    pub fn sign_in(&mut self, cx: &mut Context<Self>) {
        if self.state.registries.busy {
            return;
        }
        let Some(engine) = self.engine() else { return };
        let inputs = &self.inputs.registry;
        let server = registry::server_key(inputs.server.read(cx).text());
        let username = inputs.username.read(cx).text().trim().to_owned();
        let secret = inputs.secret.read(cx).text().to_owned();
        if secret.is_empty() || username.is_empty() {
            self.state.registries.error = Some("Enter a username and a password or token.".into());
            cx.notify();
            return;
        }
        inputs.secret.update(cx, |input, cx| input.set_text("", cx));
        self.state.registries.busy = true;
        self.state.registries.error = None;
        let auth = RegistryAuth {
            server: server.clone(),
            username,
            secret,
        };
        tracing::info!(registry = %server, "registry sign-in");
        self.spawn_run(
            cx,
            async move {
                let credentials = Arc::new(Credentials::detect());
                ops::sign_in(&engine, credentials, auth).await
            },
            move |app, result, cx| {
                app.state.registries.busy = false;
                match result {
                    Ok(store) => {
                        app.notify(
                            format!(
                                "Signed in to {} · stored in {}",
                                registry::display_name(&server),
                                store.label()
                            ),
                            false,
                            cx,
                        );
                        app.load_registries(cx);
                    }
                    Err(error) => app.state.registries.error = Some(error),
                }
            },
        );
        cx.notify();
    }

    /// `docker logout`: erases the stored credentials of `server`.
    pub fn sign_out(&mut self, server: String, cx: &mut Context<Self>) {
        if self.state.registries.busy {
            return;
        }
        self.state.registries.busy = true;
        let target = server.clone();
        self.spawn_run(
            cx,
            blocking(move || Credentials::detect().erase(&target)),
            move |app, result, cx| {
                app.state.registries.busy = false;
                app.report(
                    &result,
                    format!("Signed out of {}", registry::display_name(&server)),
                    cx,
                );
                app.load_registries(cx);
            },
        );
        cx.notify();
    }
}
