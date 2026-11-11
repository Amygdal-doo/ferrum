//! "Run container" (create + start), image tag and image history.

use crate::app::FerrumContainers;
use crate::commands::resources::image_label;
use crate::state::{Dialog, LayersState, RunState, Section};
use ferrumdock_engine::create::{RestartPolicy, RunForm};
use ferrumdock_engine::pull::PullProgress;
use ferrumdock_engine::{Image, ops};
use gpui::Context;
use std::path::PathBuf;

impl FerrumContainers {
    /// Opens the run dialog; `image` pre-fills the image field (from the images list).
    pub fn open_run_dialog(&mut self, image: Option<String>, cx: &mut Context<Self>) {
        if let Some(image) = image {
            self.inputs
                .run
                .image
                .update(cx, |input, cx| input.set_text(image, cx));
        }
        if !self.state.run.running {
            self.state.run.status = None;
            self.state.run.error = None;
        }
        self.state.dialog = Some(Dialog::Run);
        cx.notify();
    }

    pub fn set_restart(&mut self, restart: RestartPolicy, cx: &mut Context<Self>) {
        self.state.run.restart = restart;
        cx.notify();
    }

    pub fn toggle_detached(&mut self, cx: &mut Context<Self>) {
        self.state.run.detached = !self.state.run.detached;
        cx.notify();
    }

    fn run_form(&self, cx: &Context<Self>) -> RunForm {
        let text = |input: &gpui::Entity<ferrum_ui::TextInput>| input.read(cx).text().to_owned();
        let run = &self.inputs.run;
        RunForm {
            image: text(&run.image),
            name: text(&run.name),
            ports: text(&run.ports),
            env: text(&run.env),
            volumes: text(&run.volumes),
            command: text(&run.command),
            restart: self.state.run.restart,
        }
    }

    /// Validates the form, then creates and starts the container (pulling
    /// the image first when it is missing). Without "Detached" the new
    /// container's logs open.
    pub fn submit_run(&mut self, cx: &mut Context<Self>) {
        if self.state.run.running {
            return;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let spec = match self.run_form(cx).parse(home.as_deref()) {
            Ok(spec) => spec,
            Err(error) => {
                self.state.run.error = Some(error);
                cx.notify();
                return;
            }
        };
        let Some(engine) = self.engine() else { return };
        let detached = self.state.run.detached;
        self.state.run = RunState {
            restart: spec.restart,
            detached,
            running: true,
            status: Some(format!("Creating {}…", spec.image)),
            error: None,
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<PullProgress>();
        let producer = self.services.spawn(async move {
            ops::run_container(&engine, &spec, |p| {
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
                    app.state.run.status = Some(format!("Pulling · {}", progress.summary()));
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
                app.state.run.running = false;
                app.state.run.status = None;
                match result {
                    Ok(id) => {
                        if matches!(app.state.dialog, Some(Dialog::Run)) {
                            app.state.dialog = None;
                        }
                        app.notify("Container started", false, cx);
                        app.refresh(Section::Images, cx);
                        if !detached {
                            app.select_section(Section::Containers, cx);
                            app.select_container(&id, cx);
                            if let Some(detail) = app.state.detail.as_mut() {
                                detail.grace = 2;
                            }
                        }
                    }
                    Err(error) => app.state.run.error = Some(error),
                }
                app.refresh(Section::Containers, cx);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub fn open_tag_dialog(&mut self, image: &Image, cx: &mut Context<Self>) {
        let current = image.tags.first().cloned().unwrap_or_default();
        self.inputs
            .tag
            .update(cx, |input, cx| input.set_text(current, cx));
        self.state.dialog = Some(Dialog::Tag(image.clone()));
        cx.notify();
    }

    /// `docker tag <image> <reference>` from the tag dialog.
    pub fn submit_tag(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Tag(image)) = self.state.dialog.as_ref() else {
            return;
        };
        let reference = self.inputs.tag.read(cx).text().trim().to_owned();
        let Some((repo, tag)) = ops::tag_reference(&reference) else {
            self.notify(
                format!("{reference:?} is not a valid repository[:tag] (lowercase repository, tag of letters, digits, '_', '.', '-')"),
                true,
                cx,
            );
            return;
        };
        let id = image.id.clone();
        let label = image_label(image);
        self.state.dialog = None;
        self.state.busy.insert(id.clone());
        let target = id.clone();
        self.with_engine(
            cx,
            move |e| async move { e.tag_image(&target, &repo, &tag).await },
            move |app, result, cx| {
                app.state.busy.remove(&id);
                app.report(&result, format!("Tagged {label} as {reference}"), cx);
                app.refresh(Section::Images, cx);
            },
        );
        cx.notify();
    }

    /// Opens the history dialog and loads the layers.
    pub fn open_image_history(&mut self, image: &Image, cx: &mut Context<Self>) {
        let label = image_label(image);
        self.state.layers = Some(LayersState {
            label: label.clone(),
            layers: None,
        });
        self.state.dialog = Some(Dialog::History);
        let id = image.id.clone();
        self.with_engine(
            cx,
            move |e| async move { e.image_history(&id).await },
            move |app, result, _| {
                if let Some(layers) = app.state.layers.as_mut().filter(|l| l.label == label) {
                    layers.layers = Some(result);
                }
            },
        );
        cx.notify();
    }
}
