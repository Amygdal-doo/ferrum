//! Application-level operations on top of [`Engine`]: the policies the UI
//! relies on, kept here so they are tested without a window.

use crate::create::CreateSpec;
use crate::credentials::Credentials;
use crate::engine::SharedEngine;
use crate::error::{EngineError, EngineResult};
use crate::format;
use crate::model::{Container, ContainerAction, ContainerState, PruneReport};
use crate::pull::PullProgress;
use crate::registry::{self, RegistryAuth, SecretStore};
use futures::StreamExt;

/// Lifecycle buttons offered for a container in `state` (remove is separate).
#[must_use]
pub fn lifecycle_actions(state: ContainerState) -> Vec<ContainerAction> {
    let mut actions = Vec::new();
    if state.can_start() {
        actions.push(ContainerAction::Start);
    }
    if state.can_stop() {
        actions.push(ContainerAction::Stop);
    }
    if state.is_up() || state == ContainerState::Paused {
        actions.push(ContainerAction::Restart);
    }
    if state.can_pause() {
        actions.push(ContainerAction::Pause);
    }
    if state.can_unpause() {
        actions.push(ContainerAction::Unpause);
    }
    actions
}

/// Removing a running/paused container needs `force` (the confirm says so).
#[must_use]
pub fn remove_action(container: &Container) -> ContainerAction {
    ContainerAction::Remove {
        force: container.state.can_stop(),
    }
}

/// `Removed 2 images · reclaimed 100 B`, or `Nothing to remove`.
#[must_use]
pub fn prune_message(noun: &str, report: &PruneReport) -> String {
    let n = report.deleted.len();
    if n == 0 && report.reclaimed == 0 {
        return format!("No unused {noun}s to remove");
    }
    let mut message = format!("Removed {n} {noun}{}", if n == 1 { "" } else { "s" });
    if report.reclaimed > 0 {
        message.push_str(" · reclaimed ");
        message.push_str(&format::bytes(report.reclaimed));
    }
    message
}

/// Pulls `reference`, calling `on_progress` after every event. Fails with the
/// engine's message when the pull reports an error mid-stream.
pub async fn pull(
    engine: &SharedEngine,
    reference: &str,
    auth: Option<RegistryAuth>,
    mut on_progress: impl FnMut(&PullProgress) + Send,
) -> EngineResult<PullProgress> {
    let mut progress = PullProgress::default();
    let mut stream = engine.pull(reference, auth);
    while let Some(event) = stream.next().await {
        progress.apply(&event?);
        if let Some(error) = &progress.error {
            let message = error.clone();
            on_progress(&progress);
            return Err(EngineError::Api {
                status: 500,
                message,
            });
        }
        on_progress(&progress);
    }
    progress.finished = true;
    on_progress(&progress);
    Ok(progress)
}

/// `docker run -d`: creates the container (pulling the image first when the
/// engine does not have it, like the CLI) and starts it. Returns its id.
/// A container that was created but failed to start is left in place
/// (`created`), as `docker run` does, and the error says so.
pub async fn run_container(
    engine: &SharedEngine,
    spec: &CreateSpec,
    auth: Option<RegistryAuth>,
    on_pull: impl FnMut(&PullProgress) + Send,
) -> EngineResult<String> {
    let id = match engine.create_container(spec).await {
        Ok(id) => id,
        Err(error) if error.is_not_found() => {
            tracing::info!(image = %spec.image, "image not present: pulling before create");
            pull(engine, &spec.image, auth, on_pull).await?;
            engine.create_container(spec).await?
        }
        Err(error) => return Err(error),
    };
    if let Err(error) = engine.container_action(&id, ContainerAction::Start).await {
        return Err(EngineError::Api {
            status: 500,
            message: format!(
                "Created {} but it did not start: {error}",
                format::short_id(&id)
            ),
        });
    }
    Ok(id)
}

/// `docker login`: the engine checks `auth` with the registry, then the
/// credentials are stored (the identity token instead of the password when
/// the registry returns one). Returns where they were stored.
pub async fn sign_in(
    engine: &SharedEngine,
    credentials: std::sync::Arc<Credentials>,
    auth: RegistryAuth,
) -> EngineResult<SecretStore> {
    if !registry::valid_server(&auth.server) || auth.username.trim().is_empty() {
        return Err(EngineError::Api {
            status: 400,
            message: "A registry, a username and a password or token are needed".into(),
        });
    }
    let outcome = engine.check_auth(&auth).await?;
    let stored = match outcome.identity_token {
        Some(token) => RegistryAuth {
            username: registry::TOKEN_USERNAME.to_owned(),
            secret: token,
            ..auth
        },
        None => auth,
    };
    tokio::task::spawn_blocking(move || credentials.save(&stored))
        .await
        .map_err(|e| EngineError::Command(e.to_string()))?
        .map_err(EngineError::Command)
}

/// A new reference for `docker tag`: (`repo`, `tag`), `latest` when no tag is
/// given. Digests cannot be used as tags.
#[must_use]
pub fn tag_reference(reference: &str) -> Option<(String, String)> {
    if reference.contains('@') {
        return None;
    }
    let (repo, tag) = crate::pull::split_reference(reference)?;
    let valid_tag = tag.len() <= 128
        && tag
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
    let valid_repo = repo == repo.to_lowercase();
    (valid_tag && valid_repo).then_some((repo, tag))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeEngine;
    use crate::model::PullEvent;
    use std::sync::Arc;

    #[test]
    fn offers_actions_by_state() {
        use ContainerAction as A;
        assert_eq!(
            lifecycle_actions(ContainerState::Running),
            [A::Stop, A::Restart, A::Pause]
        );
        assert_eq!(
            lifecycle_actions(ContainerState::Paused),
            [A::Stop, A::Restart, A::Unpause]
        );
        assert_eq!(lifecycle_actions(ContainerState::Exited), [A::Start]);
        assert_eq!(lifecycle_actions(ContainerState::Created), [A::Start]);
        assert!(lifecycle_actions(ContainerState::Removing).is_empty());
    }

    #[test]
    fn describes_prune_results() {
        let report = |deleted: &[&str], reclaimed| PruneReport {
            deleted: deleted.iter().map(|s| (*s).to_owned()).collect(),
            reclaimed,
        };
        assert_eq!(
            prune_message("volume", &report(&[], 0)),
            "No unused volumes to remove"
        );
        assert_eq!(
            prune_message("network", &report(&["a"], 0)),
            "Removed 1 network"
        );
        assert_eq!(
            prune_message("image", &report(&["a", "b"], 2048)),
            "Removed 2 images · reclaimed 2.00 KB"
        );
    }

    #[tokio::test]
    async fn removes_running_containers_with_force_only() {
        let fake = Arc::new(FakeEngine::with_sample_data());
        let engine: SharedEngine = fake.clone();
        let containers = engine.containers().await.unwrap_or_default();
        let running = containers.iter().find(|c| c.name == "web-app-1").cloned();
        let stopped = containers.iter().find(|c| c.name == "web-db-1").cloned();
        let (Some(running), Some(stopped)) = (running, stopped) else {
            panic!("fixtures");
        };
        // Without force the engine refuses, as Docker does.
        let refused = engine
            .container_action(&running.id, ContainerAction::Remove { force: false })
            .await;
        assert!(refused.is_err_and(|e| e.is_conflict()));
        for c in [&running, &stopped] {
            assert!(
                engine
                    .container_action(&c.id, remove_action(c))
                    .await
                    .is_ok()
            );
        }
        let calls = fake.state().calls.clone();
        assert_eq!(
            calls[1..],
            [
                "remove container aaa111 force=true",
                "remove container bbb222 force=false"
            ]
        );
        assert_eq!(engine.containers().await.map(|c| c.len()).ok(), Some(1));
    }

    #[tokio::test]
    async fn lifecycle_and_prunes_against_the_fake() {
        let engine: SharedEngine = Arc::new(FakeEngine::with_sample_data());
        assert!(
            engine
                .container_action("bbb222", ContainerAction::Start)
                .await
                .is_ok()
        );
        assert!(
            engine
                .container_action("bbb222", ContainerAction::Unpause)
                .await
                .is_err_and(|e| e.is_conflict())
        );
        assert!(
            engine
                .container_action("nope", ContainerAction::Stop)
                .await
                .is_err_and(|e| e.is_not_found())
        );

        let images = engine.prune_images().await.unwrap_or_default();
        assert_eq!((images.deleted.len(), images.reclaimed), (2, 100));
        assert!(
            engine
                .remove_image("sha256:img1")
                .await
                .is_err_and(|e| e.is_conflict())
        );

        let volumes = engine.prune_volumes().await.unwrap_or_default();
        assert_eq!(volumes.deleted, ["old"]);
        assert_eq!(
            prune_message("volume", &volumes),
            "Removed 1 volume · reclaimed 300 B"
        );

        let networks = engine.prune_networks().await.unwrap_or_default();
        assert_eq!(networks.deleted, ["web_default"]);
        assert!(engine.remove_network("net-bridge").await.is_err());
    }

    #[tokio::test]
    async fn pull_reports_progress_and_errors() {
        let fake = Arc::new(FakeEngine::default());
        fake.state().pull_events = vec![
            PullEvent {
                id: Some("l1".into()),
                status: "Downloading".into(),
                current: Some(5),
                total: Some(10),
                error: None,
            },
            PullEvent {
                id: Some("l1".into()),
                status: "Pull complete".into(),
                ..PullEvent::default()
            },
            PullEvent {
                status: "Status: Downloaded newer image for nginx:latest".into(),
                ..PullEvent::default()
            },
        ];
        let engine: SharedEngine = fake.clone();
        let mut seen = Vec::new();
        let result = pull(&engine, "nginx", None, |p| seen.push(p.fraction())).await;
        assert!(result.is_ok_and(|p| p.finished && p.layers() == (1, 1)));
        assert_eq!(seen.first().copied().flatten(), Some(0.5));
        assert_eq!(seen.last().copied().flatten(), Some(1.0));
        assert_eq!(fake.state().calls, ["pull nginx"]);

        fake.state().pull_events = vec![PullEvent {
            error: Some("pull access denied for nope".into()),
            ..PullEvent::default()
        }];
        let failed = pull(&engine, "nope", None, |_| {}).await;
        assert!(
            matches!(failed, Err(EngineError::Api { message, .. }) if message.contains("access denied"))
        );
    }

    #[tokio::test]
    async fn run_pulls_a_missing_image_then_starts() {
        let fake = Arc::new(FakeEngine::with_sample_data());
        let engine: SharedEngine = fake.clone();
        let spec = CreateSpec {
            image: "redis".into(),
            name: Some("cache".into()),
            ..CreateSpec::default()
        };
        let id = run_container(&engine, &spec, None, |_| {}).await;
        let id = id.unwrap_or_else(|e| panic!("run: {e}"));
        assert_eq!(
            fake.state().calls,
            [
                "create cache redis:latest",
                "pull redis",
                "create cache redis:latest",
                &format!("start {id}")
            ]
        );
        let created = fake.state().containers.iter().find(|c| c.id == id).cloned();
        assert!(created.is_some_and(|c| c.state.is_up() && c.name == "cache"));

        // Same name again: the engine's conflict comes back unchanged.
        let again = run_container(&engine, &spec, None, |_| {}).await;
        assert!(again.is_err_and(|e| e.is_conflict()));

        // A failing pull stops before create.
        fake.state().pull_events = vec![PullEvent {
            error: Some("pull access denied".into()),
            ..PullEvent::default()
        }];
        let denied = CreateSpec {
            image: "private/app".into(),
            ..CreateSpec::default()
        };
        let failed = run_container(&engine, &denied, None, |_| {}).await;
        assert!(failed.is_err_and(|e| e.to_string().contains("access denied")));
    }

    fn private_registry() -> (Arc<FakeEngine>, SharedEngine) {
        let fake = Arc::new(FakeEngine::default());
        {
            let mut s = fake.state();
            s.private_repos = vec!["ghcr.io/acme/api".into()];
            s.registry_accounts
                .insert("ghcr.io".into(), ("me".into(), "ghp_ok".into()));
        }
        let engine: SharedEngine = fake.clone();
        (fake, engine)
    }

    fn ghcr(secret: &str) -> RegistryAuth {
        RegistryAuth {
            server: "ghcr.io".into(),
            username: "me".into(),
            secret: secret.into(),
        }
    }

    #[tokio::test]
    async fn pulls_private_images_with_credentials() {
        let (fake, engine) = private_registry();
        let anonymous = pull(&engine, "ghcr.io/acme/api:1", None, |_| {}).await;
        assert!(anonymous.is_err_and(|e| e.to_string().contains("docker login")));
        let wrong = pull(&engine, "ghcr.io/acme/api:1", Some(ghcr("bad")), |_| {}).await;
        assert!(wrong.is_err());
        let signed_in = pull(&engine, "ghcr.io/acme/api:1", Some(ghcr("ghp_ok")), |_| {}).await;
        assert!(signed_in.is_ok());
        assert_eq!(
            fake.state().calls.last().map(String::as_str),
            Some("pull ghcr.io/acme/api:1 as me@ghcr.io")
        );

        // `docker run` of a private image pulls with the same credentials.
        let spec = CreateSpec {
            image: "ghcr.io/acme/api:2".into(),
            ..CreateSpec::default()
        };
        let run = run_container(&engine, &spec, Some(ghcr("ghp_ok")), |_| {}).await;
        assert!(run.is_ok(), "{run:?}");
    }

    #[tokio::test]
    async fn signs_in_only_with_accepted_credentials() {
        use crate::credentials::MemoryBackend;
        let (fake, engine) = private_registry();
        let backend = Arc::new(MemoryBackend::default());
        let credentials = Arc::new(Credentials::new(None, backend.clone()));

        let refused = sign_in(&engine, credentials.clone(), ghcr("bad")).await;
        assert!(refused.is_err_and(|e| e.to_string().contains("401")));
        assert!(credentials.list().is_empty(), "nothing stored on failure");

        let stored = sign_in(&engine, credentials.clone(), ghcr("ghp_ok")).await;
        assert_eq!(stored, Ok(SecretStore::Keychain));
        assert_eq!(credentials.get("ghcr.io"), Ok(Some(ghcr("ghp_ok"))));
        assert_eq!(fake.state().calls, ["auth ghcr.io me", "auth ghcr.io me"]);

        let empty = RegistryAuth {
            username: " ".into(),
            ..ghcr("x")
        };
        assert!(sign_in(&engine, credentials, empty).await.is_err());
        assert_eq!(fake.state().calls.len(), 2, "not sent to the engine");
    }

    #[tokio::test]
    async fn tags_and_lists_layers() {
        let fake = Arc::new(FakeEngine::with_sample_data());
        let engine: SharedEngine = fake.clone();
        assert_eq!(
            tag_reference("registry:5000/team/app"),
            Some(("registry:5000/team/app".into(), "latest".into()))
        );
        assert_eq!(
            tag_reference("app:v1.2"),
            Some(("app".into(), "v1.2".into()))
        );
        assert_eq!(tag_reference("App:v1"), None, "repositories are lowercase");
        assert_eq!(tag_reference("app@sha256:abc"), None);
        assert_eq!(tag_reference("app:-x"), None);
        assert!(engine.tag_image("sha256:img2", "app", "v1").await.is_ok());
        let images = engine.images().await.unwrap_or_default();
        assert!(
            images
                .iter()
                .any(|i| i.id == "sha256:img2" && i.tags == ["app:v1"])
        );
        assert!(
            engine
                .tag_image("nope", "a", "b")
                .await
                .is_err_and(|e| e.is_not_found())
        );
        let layers = engine
            .image_history("sha256:img1")
            .await
            .unwrap_or_default();
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0].tags, ["web-app:latest"]);
    }
}
