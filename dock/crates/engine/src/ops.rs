//! Application-level operations on top of [`Engine`]: the policies the UI
//! relies on, kept here so they are tested without a window.

use crate::engine::SharedEngine;
use crate::error::{EngineError, EngineResult};
use crate::format;
use crate::model::{Container, ContainerAction, ContainerState, PruneReport};
use crate::pull::PullProgress;
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
    mut on_progress: impl FnMut(&PullProgress) + Send,
) -> EngineResult<PullProgress> {
    let mut progress = PullProgress::default();
    let mut stream = engine.pull(reference);
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
        let result = pull(&engine, "nginx", |p| seen.push(p.fraction())).await;
        assert!(result.is_ok_and(|p| p.finished && p.layers() == (1, 1)));
        assert_eq!(seen.first().copied().flatten(), Some(0.5));
        assert_eq!(seen.last().copied().flatten(), Some(1.0));
        assert_eq!(fake.state().calls, ["pull nginx"]);

        fake.state().pull_events = vec![PullEvent {
            error: Some("pull access denied for nope".into()),
            ..PullEvent::default()
        }];
        let failed = pull(&engine, "nope", |_| {}).await;
        assert!(
            matches!(failed, Err(EngineError::Api { message, .. }) if message.contains("access denied"))
        );
    }
}
