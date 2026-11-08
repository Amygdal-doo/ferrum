//! Environment and globals mutations.

use crate::app::FerrumApi;
use crate::state::Dialog;
use ferrumapi_domain::{Environment, EnvironmentId};
use gpui::Context;

impl FerrumApi {
    fn persist_environment(&mut self, env: Environment, cx: &mut Context<Self>) {
        let svc = self.services.clone();
        self.io(
            cx,
            async move {
                if env.id.is_globals() {
                    svc.store.save_globals(&env).await
                } else {
                    svc.store.save_environment(&env).await
                }
            },
            Self::report("Could not save environment"),
        );
    }

    /// Stores `env` (globals or a regular environment) in memory and on disk.
    pub fn save_environment(&mut self, env: Environment, cx: &mut Context<Self>) {
        if env.id.is_globals() {
            self.state.globals = env.clone();
        } else {
            let list = &mut self.state.environments;
            match list.iter_mut().find(|e| e.id == env.id) {
                Some(existing) => *existing = env.clone(),
                None => list.push(env.clone()),
            }
        }
        self.persist_environment(env, cx);
    }

    #[must_use]
    pub fn environment(&self, id: EnvironmentId) -> Option<Environment> {
        if id.is_globals() {
            Some(self.state.globals.clone())
        } else {
            self.state.environments.iter().find(|e| e.id == id).cloned()
        }
    }

    pub fn edit_environment(&mut self, id: EnvironmentId) {
        if let Some(env) = self.environment(id) {
            self.state.dialog = Some(Dialog::EditEnvironment(Box::new(env)));
        }
    }

    pub fn create_environment(&mut self, cx: &mut Context<Self>) {
        let env = Environment::new("New Environment");
        let id = env.id;
        self.save_environment(env, cx);
        self.edit_environment(id);
    }

    pub fn rename_environment(&mut self, id: EnvironmentId, name: String, cx: &mut Context<Self>) {
        let name = name.trim().to_owned();
        if name.is_empty() || id.is_globals() {
            return;
        }
        if let Some(mut env) = self.environment(id) {
            env.name = name;
            self.save_environment(env, cx);
        }
    }

    pub fn duplicate_environment(&mut self, id: EnvironmentId, cx: &mut Context<Self>) {
        if let Some(env) = self.environment(id) {
            let mut copy = Environment::new(format!("{} Copy", env.name));
            copy.variables = env.variables;
            self.save_environment(copy, cx);
        }
    }

    pub fn delete_environment(&mut self, id: EnvironmentId, cx: &mut Context<Self>) {
        if id.is_globals() {
            return;
        }
        self.state.environments.retain(|e| e.id != id);
        if self.state.active_env == Some(id) {
            self.set_active_environment(None);
        }
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.store.delete_environment(id).await },
            Self::report("Could not delete environment"),
        );
    }

    pub fn set_active_environment(&mut self, id: Option<EnvironmentId>) {
        self.state.active_env = id;
        self.state.touch();
    }
}
