//! Profile CRUD and connect/disconnect.

use super::background;
use crate::state::{AppState, ConnStatus};
use dioxus::prelude::*;
use ferrumdata_application::{AppServices, PasswordChange};
use ferrumdata_domain::{ConnectionProfile, ProfileId};

pub async fn reload_profiles(mut state: AppState, services: AppServices) {
    let svc = services.clone();
    if let Ok(Ok(profiles)) = background(
        &services,
        async move { svc.connections.list_profiles().await },
    )
    .await
    {
        state.profiles.set(profiles);
    }
}

/// Saves a profile; returns an error message for the form on failure.
pub async fn save(
    state: AppState,
    services: AppServices,
    profile: ConnectionProfile,
    password: PasswordChange,
) -> Result<ConnectionProfile, String> {
    let svc = services.clone();
    let saved = background(&services, async move {
        svc.connections.save_profile(profile, password).await
    })
    .await?
    .map_err(|e| e.message)?;
    let mut status = state.conn_status;
    status.write().remove(&saved.id);
    reload_profiles(state, services).await;
    Ok(saved)
}

pub fn delete(mut state: AppState, services: AppServices, id: ProfileId) {
    spawn(async move {
        let svc = services.clone();
        match background(&services, async move {
            svc.connections.delete_profile(id).await
        })
        .await
        {
            Ok(Ok(())) => {
                state.conn_status.write().remove(&id);
                for tab in state
                    .tabs
                    .write()
                    .iter_mut()
                    .filter(|t| t.profile_id == Some(id))
                {
                    tab.profile_id = None;
                    tab.unsaved = true;
                }
                reload_profiles(state, services).await;
            }
            Ok(Err(e)) => state.notify(e.message, true),
            Err(e) => state.notify(e, true),
        }
    });
}

pub fn connect(mut state: AppState, services: AppServices, id: ProfileId) {
    state.conn_status.write().insert(id, ConnStatus::Connecting);
    spawn(async move {
        let svc = services.clone();
        let result = background(&services, async move { svc.connections.connect(id).await }).await;
        let status = match result {
            Ok(Ok(info)) => ConnStatus::Ready(info.version),
            Ok(Err(e)) => ConnStatus::Failed(e.message),
            Err(e) => ConnStatus::Failed(e),
        };
        state.conn_status.write().insert(id, status);
    });
}

pub fn disconnect(mut state: AppState, services: AppServices, id: ProfileId) {
    state.conn_status.write().remove(&id);
    let svc = services.clone();
    services.spawn(async move { svc.connections.disconnect(id).await });
}
