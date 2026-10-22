//! Environment and globals mutations.

use super::run;
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::{Environment, EnvironmentId};

fn persist(state: AppState, services: AppServices, env: Environment) {
    let svc = services.clone();
    spawn(async move {
        let result = run(&services, async move {
            if env.id.is_globals() {
                svc.store.save_globals(&env).await
            } else {
                svc.store.save_environment(&env).await
            }
        })
        .await;
        if let Err(e) = result {
            state.notify(format!("Could not save environment: {e}"), true);
        }
    });
}

/// Stores `env` (globals or a regular environment) in memory and on disk.
pub fn save(mut state: AppState, services: AppServices, env: Environment) {
    if env.id.is_globals() {
        state.globals.set(env.clone());
    } else {
        let mut list = state.environments.write();
        match list.iter_mut().find(|e| e.id == env.id) {
            Some(existing) => *existing = env.clone(),
            None => list.push(env.clone()),
        }
    }
    persist(state, services, env);
}

#[must_use]
pub fn get(state: AppState, id: EnvironmentId) -> Option<Environment> {
    if id.is_globals() {
        Some(state.globals.peek().clone())
    } else {
        state
            .environments
            .peek()
            .iter()
            .find(|e| e.id == id)
            .cloned()
    }
}

pub fn create(mut state: AppState, services: AppServices) {
    let env = Environment::new("New Environment");
    let id = env.id;
    save(state, services, env);
    state.dialog.set(Some(Dialog::EditEnvironment(id)));
}

pub fn rename(state: AppState, services: AppServices, id: EnvironmentId, name: String) {
    let name = name.trim().to_owned();
    if name.is_empty() || id.is_globals() {
        return;
    }
    if let Some(mut env) = get(state, id) {
        env.name = name;
        save(state, services, env);
    }
}

pub fn duplicate(state: AppState, services: AppServices, id: EnvironmentId) {
    if let Some(env) = get(state, id) {
        let mut copy = Environment::new(format!("{} Copy", env.name));
        copy.variables = env.variables;
        save(state, services, copy);
    }
}

pub fn delete(mut state: AppState, services: AppServices, id: EnvironmentId) {
    if id.is_globals() {
        return;
    }
    state.environments.write().retain(|e| e.id != id);
    if *state.active_env.peek() == Some(id) {
        set_active(state, None);
    }
    let svc = services.clone();
    spawn(async move {
        if let Err(e) = run(
            &services,
            async move { svc.store.delete_environment(id).await },
        )
        .await
        {
            state.notify(format!("Could not delete environment: {e}"), true);
        }
    });
}

pub fn set_active(mut state: AppState, id: Option<EnvironmentId>) {
    state.active_env.set(id);
    state.touch();
}
