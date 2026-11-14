//! The VM the engine runs in: Start / Stop buttons, and stopping it on its
//! own after `auto_stop_minutes` with no running container (`vm.json`, the
//! same setting `fec autostop` changes).

use crate::app::FerrumContainers;
use crate::state::EngineStatus;
use ferrumdock_engine::vm::{Vm, VmSettings};
use gpui::Context;
use std::time::{Duration, Instant};

impl FerrumContainers {
    /// The VM behind the connected engine (none for a remote or plain Linux engine).
    pub fn connected_vm(&self) -> Option<Vm> {
        match &self.status {
            EngineStatus::Connected(c) => Vm::for_candidate(&c.candidate),
            _ => None,
        }
    }

    /// Called after each container poll: counts idle time and stops the VM
    /// once it has had no running container for the configured minutes.
    pub(crate) fn track_idle(&mut self, any_running: bool, cx: &mut Context<Self>) {
        if any_running {
            self.idle_since = None;
            return;
        }
        let since = *self.idle_since.get_or_insert_with(Instant::now);
        let minutes = VmSettings::load().auto_stop_minutes;
        if minutes == 0 || self.vm_busy {
            return;
        }
        if since.elapsed() >= Duration::from_secs(u64::from(minutes) * 60)
            && let Some(vm) = self.connected_vm()
        {
            tracing::info!(vm = vm.key(), minutes, "stopping idle VM");
            self.stop_vm(vm, Some(minutes), cx);
        }
    }

    /// Starts `vm`, then connects to it.
    pub fn start_vm(&mut self, vm: Vm, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.vm_busy, true) {
            return;
        }
        self.notify(format!("Starting {}…", vm.name()), false, cx);
        let work = async move {
            tokio::task::spawn_blocking(move || vm.start(&[]))
                .await
                .map_err(|e| e.to_string())?
        };
        self.spawn_run(cx, work, move |app, result, cx| {
            app.vm_busy = false;
            app.idle_since = None;
            match result {
                Ok(()) => {
                    app.notify(format!("{} is running", vm.name()), false, cx);
                    app.connect(false, cx);
                }
                Err(e) => app.notify(format!("Could not start {}: {e}", vm.name()), true, cx),
            }
        });
    }

    /// Stops `vm`; `idle_minutes` when it was stopped for being idle.
    pub fn stop_vm(&mut self, vm: Vm, idle_minutes: Option<u32>, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.vm_busy, true) {
            return;
        }
        self.notify(format!("Stopping {}…", vm.name()), false, cx);
        let work = async move {
            tokio::task::spawn_blocking(move || vm.stop(&[]))
                .await
                .map_err(|e| e.to_string())?
        };
        self.spawn_run(cx, work, move |app, result, cx| {
            app.vm_busy = false;
            app.idle_since = None;
            match result {
                Ok(()) => {
                    let message = match idle_minutes {
                        Some(m) => format!(
                            "Stopped {}: no container ran for {m} min. Start it again here or with `fec start`.",
                            vm.name()
                        ),
                        None => format!("{} stopped", vm.name()),
                    };
                    app.notify(message, false, cx);
                    app.connect(false, cx);
                }
                Err(e) => app.notify(format!("Could not stop {}: {e}", vm.name()), true, cx),
            }
        });
    }
}
