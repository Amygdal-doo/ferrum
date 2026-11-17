//! Connecting to the engine, refreshing lists and the live poll loop.

use crate::app::FerrumContainers;
use crate::state::{EngineStatus, Section};
use ferrumdock_engine::{EngineError, sample_stats};
use gpui::{Context, Task};
use std::convert::Infallible;
use std::time::Duration;

/// Containers + stats refresh period.
const POLL: Duration = Duration::from_secs(2);
/// While offline, try to reconnect every this many polls.
const RECONNECT_EVERY: u32 = 3;

impl FerrumContainers {
    /// (Re)connects; `quiet` keeps the current screen until it succeeds.
    pub fn connect(&mut self, quiet: bool, cx: &mut Context<Self>) {
        if self.connecting {
            return;
        }
        self.connecting = true;
        if !quiet {
            self.status = EngineStatus::Connecting;
            cx.notify();
        }
        let preferred = self.state.preferred.clone();
        self.spawn_run(
            cx,
            ferrumdock_engine::connect(preferred),
            move |app, result, cx| {
            app.connecting = false;
            match result {
                Ok(connection) => {
                    tracing::info!(endpoint = %connection.candidate.endpoint, version = %connection.info.version, "connected");
                    app.state.clear_engine_data();
                    app.stop_logs();
                    app.status = EngineStatus::Connected(connection);
                    app.refresh(Section::Containers, cx);
                    if app.state.section != Section::Containers {
                        app.refresh(app.state.section, cx);
                    }
                }
                Err(error) => {
                    if !quiet {
                        tracing::info!(%error, "no engine");
                    }
                    app.go_offline(error);
                }
            }
            },
        );
    }

    fn go_offline(&mut self, error: String) {
        self.state.clear_engine_data();
        self.stop_logs();
        self.status = EngineStatus::Offline(error);
    }

    /// Reloads one section's list.
    pub fn refresh(&mut self, section: Section, cx: &mut Context<Self>) {
        match section {
            Section::Containers => {
                self.reload_project_files();
                self.poll_containers(cx);
            }
            Section::Images => self.with_engine(
                cx,
                |e| async move { e.images().await },
                |app, r, cx| {
                    match r {
                        Ok(list) => app.state.images = list,
                        Err(error) => app.notify(error, true, cx),
                    }
                    app.state.loaded.insert(Section::Images);
                },
            ),
            Section::Volumes => self.with_engine(
                cx,
                |e| async move { e.volumes().await },
                |app, r, cx| {
                    match r {
                        Ok(list) => app.state.volumes = list,
                        Err(error) => app.notify(error, true, cx),
                    }
                    app.state.loaded.insert(Section::Volumes);
                },
            ),
            Section::Kubernetes => self.load_kube(cx),
            Section::Networks => self.with_engine(
                cx,
                |e| async move { e.networks().await },
                |app, r, cx| {
                    match r {
                        Ok(list) => app.state.networks = list,
                        Err(error) => app.notify(error, true, cx),
                    }
                    app.state.loaded.insert(Section::Networks);
                },
            ),
        }
    }

    /// Lists containers and, when they are on screen, samples stats of the running ones.
    /// A transport failure means the engine went away: show the offline screen.
    fn poll_containers(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = self.engine() else { return };
        if self.polling {
            return;
        }
        self.polling = true;
        let with_stats = self.state.section == Section::Containers;
        let work = async move {
            let result = async {
                let list = engine.containers().await?;
                let running: Vec<String> = list
                    .iter()
                    .filter(|c| c.state.is_up())
                    .map(|c| c.id.clone())
                    .collect();
                let stats = if with_stats {
                    sample_stats(&engine, &running).await
                } else {
                    Default::default()
                };
                Ok::<_, EngineError>((list, stats))
            }
            .await;
            Ok::<_, Infallible>(result)
        };
        self.spawn_run(cx, work, move |app, result, cx| {
            app.polling = false;
            let Ok(result) = result else { return };
            match result {
                Ok((list, stats)) => {
                    if with_stats {
                        app.state.stats = stats;
                        app.state.record_stats(app.started.elapsed().as_secs_f64());
                    }
                    let any_running = list.iter().any(|c| c.state.is_up());
                    app.state.containers = list;
                    app.state.loaded.insert(Section::Containers);
                    app.track_idle(any_running, cx);
                    app.sync_detail(cx);
                }
                Err(EngineError::Api { message, .. }) => app.notify(message, true, cx),
                Err(error) => {
                    tracing::warn!(%error, "engine connection lost");
                    app.go_offline(error.to_string());
                }
            }
        });
    }

    /// Ticks every [`POLL`]: refreshes containers while connected, retries while offline.
    pub fn start_polling(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let mut tick: u32 = 0;
            loop {
                cx.background_executor().timer(POLL).await;
                tick = tick.wrapping_add(1);
                let alive = this.update(cx, |app, cx| match app.status {
                    EngineStatus::Connected(_) => app.poll_containers(cx),
                    EngineStatus::Offline(_) if tick.is_multiple_of(RECONNECT_EVERY) => {
                        app.connect(true, cx)
                    }
                    _ => {}
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    pub fn select_section(&mut self, section: Section, cx: &mut Context<Self>) {
        if self.state.section != section {
            self.state.section = section;
            self.state.filter.clear();
            self.inputs
                .filter
                .update(cx, |input, cx| input.set_text("", cx));
        }
        self.refresh(section, cx);
        cx.notify();
    }
}
