//! Images (pull, remove, prune), volumes and networks; confirm dialog outcomes.

use crate::app::FerrumContainers;
use crate::state::{Confirm, Dialog, PendingAction, PullState, Section};
use ferrumdock_engine::compose::ComposeAction;
use ferrumdock_engine::pull::PullProgress;
use ferrumdock_engine::{Image, Network, PruneReport, format, ops};
use gpui::Context;

impl FerrumContainers {
    /// Runs the action of the open confirm dialog.
    pub fn accept_confirm(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Confirm(confirm)) = self.state.dialog.take() else {
            return;
        };
        let Confirm { action, .. } = *confirm;
        match action {
            PendingAction::RemoveContainer(c) => {
                self.container_action(&c, ops::remove_action(&c), cx)
            }
            PendingAction::ComposeDown(project) => {
                self.run_compose(project, ComposeAction::Down, cx)
            }
            PendingAction::RemoveImage(image) => {
                let id = image.id.clone();
                let label = image_label(&image);
                self.mutate(
                    Section::Images,
                    id.clone(),
                    move |e| async move { e.remove_image(&id).await },
                    format!("Removed {label}"),
                    cx,
                );
            }
            PendingAction::RemoveVolume(name) => {
                let target = name.clone();
                self.mutate(
                    Section::Volumes,
                    name.clone(),
                    move |e| async move { e.remove_volume(&target).await },
                    format!("Removed volume {name}"),
                    cx,
                );
            }
            PendingAction::RemoveNetwork(network) => {
                let id = network.id.clone();
                self.mutate(
                    Section::Networks,
                    network.id.clone(),
                    move |e| async move { e.remove_network(&id).await },
                    format!("Removed network {}", network.name),
                    cx,
                );
            }
            PendingAction::PruneImages => self.prune(Section::Images, "dangling image", cx),
            PendingAction::PruneVolumes => self.prune(Section::Volumes, "volume", cx),
            PendingAction::PruneNetworks => self.prune(Section::Networks, "network", cx),
        }
        cx.notify();
    }

    /// One remove call: `busy` while running, notice, then refresh `section`.
    fn mutate<F, Fut>(
        &mut self,
        section: Section,
        key: String,
        work: F,
        ok: String,
        cx: &mut Context<Self>,
    ) where
        F: FnOnce(ferrumdock_engine::SharedEngine) -> Fut,
        Fut: std::future::Future<Output = ferrumdock_engine::EngineResult<()>> + Send + 'static,
    {
        self.state.busy.insert(key.clone());
        self.with_engine(cx, work, move |app, result, cx| {
            app.state.busy.remove(&key);
            app.report(&result, ok, cx);
            app.refresh(section, cx);
        });
    }

    fn prune(&mut self, section: Section, noun: &'static str, cx: &mut Context<Self>) {
        let key = format!("prune:{}", section.title());
        self.state.busy.insert(key.clone());
        self.with_engine(
            cx,
            move |e| async move {
                match section {
                    Section::Images => e.prune_images().await,
                    Section::Volumes => e.prune_volumes().await,
                    Section::Networks => e.prune_networks().await,
                    Section::Containers => Ok(PruneReport::default()),
                }
            },
            move |app, result, cx| {
                app.state.busy.remove(&key);
                match result {
                    Ok(report) => app.notify(ops::prune_message(noun, &report), false, cx),
                    Err(error) => app.notify(error, true, cx),
                }
                app.refresh(section, cx);
            },
        );
    }

    pub fn confirm_remove_image(&mut self, image: &Image, cx: &mut Context<Self>) {
        self.ask(
            Confirm {
                title: "Remove image".into(),
                body: format!(
                    "Remove {} ({})?",
                    image_label(image),
                    format::bytes(image.size)
                ),
                confirm_label: "Remove".into(),
                action: PendingAction::RemoveImage(image.clone()),
            },
            cx,
        );
    }

    pub fn confirm_prune(&mut self, section: Section, cx: &mut Context<Self>) {
        let (title, body, action) = match section {
            Section::Images => (
                "Prune dangling images",
                "Removes untagged images not used by any container.".to_owned(),
                PendingAction::PruneImages,
            ),
            Section::Volumes => {
                let unused: Vec<_> = self
                    .state
                    .volumes
                    .iter()
                    .filter(|v| v.ref_count == Some(0))
                    .collect();
                let size: u64 = unused.iter().filter_map(|v| v.size).sum();
                let estimate = if unused.is_empty() {
                    String::new()
                } else {
                    format!(
                        " About {} volume(s), {}.",
                        unused.len(),
                        format::bytes(size)
                    )
                };
                (
                    "Prune unused volumes",
                    format!(
                        "Permanently deletes every volume not used by a container, named ones included, with their data.{estimate}"
                    ),
                    PendingAction::PruneVolumes,
                )
            }
            Section::Networks => (
                "Prune unused networks",
                "Removes custom networks not used by any container.".to_owned(),
                PendingAction::PruneNetworks,
            ),
            Section::Containers => return,
        };
        self.ask(
            Confirm {
                title: title.into(),
                body,
                confirm_label: "Prune".into(),
                action,
            },
            cx,
        );
    }

    pub fn confirm_remove_volume(&mut self, name: &str, cx: &mut Context<Self>) {
        self.ask(
            Confirm {
                title: "Remove volume".into(),
                body: format!("Permanently delete volume {name} and its data?"),
                confirm_label: "Remove".into(),
                action: PendingAction::RemoveVolume(name.to_owned()),
            },
            cx,
        );
    }

    pub fn confirm_remove_network(&mut self, network: &Network, cx: &mut Context<Self>) {
        self.ask(
            Confirm {
                title: "Remove network".into(),
                body: format!("Remove network {}?", network.name),
                confirm_label: "Remove".into(),
                action: PendingAction::RemoveNetwork(network.clone()),
            },
            cx,
        );
    }

    pub fn open_pull_dialog(&mut self, cx: &mut Context<Self>) {
        self.state.dialog = Some(Dialog::Pull);
        cx.notify();
    }

    /// Pulls the reference typed in the pull dialog; progress streams into `state.pull`.
    pub fn start_pull(&mut self, cx: &mut Context<Self>) {
        let reference = self.inputs.pull.read(cx).text().trim().to_owned();
        if reference.is_empty() || self.state.pull.as_ref().is_some_and(|p| p.running) {
            return;
        }
        let Some(engine) = self.engine() else { return };
        self.state.pull = Some(PullState {
            reference: reference.clone(),
            running: true,
            ..PullState::default()
        });
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<PullProgress>();
        let target = reference.clone();
        let producer = self.services.spawn(async move {
            let auth = ferrumdock_engine::credentials::for_reference(&target).await;
            ops::pull(&engine, &target, auth, |p| {
                let _ = tx.send(p.clone());
            })
            .await
        });
        cx.spawn(async move |this, cx| {
            while let Some(mut progress) = rx.recv().await {
                while let Ok(newer) = rx.try_recv() {
                    progress = newer;
                }
                let alive = this.update(cx, |app, cx| {
                    if let Some(pull) = app.state.pull.as_mut() {
                        pull.progress = progress;
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
            let result = match producer.await {
                Ok(r) => r.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            let _ = this.update(cx, |app, cx| {
                if let Some(pull) = app.state.pull.as_mut() {
                    pull.running = false;
                    pull.error = result.as_ref().err().cloned();
                }
                app.report(&result, format!("Pulled {reference}"), cx);
                app.refresh(Section::Images, cx);
            });
        })
        .detach();
        cx.notify();
    }
}

pub fn image_label(image: &Image) -> String {
    image
        .tags
        .first()
        .cloned()
        .unwrap_or_else(|| format!("<none> {}", image.short_id()))
}
