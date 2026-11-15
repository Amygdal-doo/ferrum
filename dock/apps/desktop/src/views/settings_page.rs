//! Settings › Engine: the VMs installed here (start / stop), which one `fec`
//! uses, the idle auto-stop, and the `fec` commands.

use std::cell::RefCell;

use ferrum_ui::components::{
    Button, ButtonCommon, ButtonSize, ButtonStyle, Clickable, Disableable, IconName,
};
use ferrum_ui::gpui::{
    self, AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement,
    Render, SharedString, Styled, Window, div, prelude::*, px,
};
use ferrum_ui::settings::SettingsPage;
use ferrum_ui::settings::widgets::{Row, Section, select};
use ferrum_ui::theme::ActiveTheme;
use ferrumdock_engine::vm::{self, Vm, VmSettings};

/// Auto-stop choices in minutes (0: never).
const AUTO_STOP: [u32; 6] = [0, 10, 15, 30, 60, 120];
const AUTO_STOP_LABELS: [&str; 6] = ["Never", "10 min", "15 min", "30 min", "1 hour", "2 hours"];

/// `fec` commands shown on the page: (command, what it does).
const COMMANDS: [(&str, &str); 9] = [
    (
        "fec start",
        "Start the VM (args go to its CLI, e.g. --cpu 4)",
    ),
    ("fec stop", "Stop the VM and every container in it"),
    (
        "fec status",
        "Which VM, whether it runs, the running containers",
    ),
    (
        "fec vm colima",
        "Choose the VM: colima, orbstack, desktop, podman or auto",
    ),
    (
        "fec autostop 30",
        "Stop the VM after 30 idle minutes (or off)",
    ),
    (
        "fec up / fec down",
        "docker compose up -d / down, starting the VM first",
    ),
    (
        "fec ps, fec logs -f web",
        "Any other command runs docker against the VM",
    ),
    (
        "fec start web",
        "With a name, start/stop/restart act on containers",
    ),
    ("fec .", "Open FerrumContainers on this folder"),
];

#[derive(Default)]
pub struct EnginePage {
    view: RefCell<Option<Entity<EngineSettingsView>>>,
}

impl SettingsPage for EnginePage {
    fn id(&self) -> &'static str {
        "engine"
    }
    fn title(&self) -> &'static str {
        "Engine"
    }
    fn icon(&self) -> IconName {
        IconName::Server
    }
    fn tint(&self) -> Hsla {
        gpui::rgb(0x2496ed).into()
    }
    fn keywords(&self) -> &'static [&'static str] {
        &[
            "vm", "colima", "orbstack", "docker", "desktop", "podman", "stop", "start", "idle",
            "auto", "fec", "cli",
        ]
    }
    fn render(&self, _: &mut Window, cx: &mut App) -> AnyElement {
        self.view
            .borrow_mut()
            .get_or_insert_with(|| cx.new(EngineSettingsView::new))
            .clone()
            .into_any_element()
    }
}

pub struct EngineSettingsView {
    /// Installed VMs and whether each is running.
    vms: Vec<(Vm, bool)>,
    /// A VM being started or stopped.
    busy: Option<Vm>,
    message: Option<String>,
}

impl EngineSettingsView {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            vms: Vec::new(),
            busy: None,
            message: None,
        };
        view.reload(cx);
        view
    }

    /// Looks again at which VMs are installed and running (off the UI thread).
    fn reload(&mut self, cx: &mut Context<Self>) {
        let task = cx.background_spawn(async {
            vm::installed()
                .into_iter()
                .map(|vm| (vm, vm.is_running()))
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let vms = task.await;
            this.update(cx, |view, cx| {
                view.vms = vms;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn toggle(&mut self, vm: Vm, running: bool, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(vm);
        self.message = Some(format!(
            "{} {}…",
            if running { "Stopping" } else { "Starting" },
            vm.name()
        ));
        cx.notify();
        let task =
            cx.background_spawn(async move { if running { vm.stop(&[]) } else { vm.start(&[]) } });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |view, cx| {
                view.busy = None;
                view.message = Some(match result {
                    Ok(()) if running => format!("{} stopped", vm.name()),
                    Ok(()) => format!("{} is running", vm.name()),
                    Err(e) => format!("{}: {e}", vm.name()),
                });
                view.reload(cx);
            })
            .ok();
        })
        .detach();
    }
}

fn update(f: impl FnOnce(&mut VmSettings)) {
    let mut settings = VmSettings::load();
    f(&mut settings);
    if let Err(error) = settings.save() {
        tracing::warn!(%error, "could not save vm.json");
    }
}

impl Render for EngineSettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let settings = VmSettings::load();

        let mut vms = Section::new("Virtual machine").detail(
            "Containers on a Mac run inside one of these. Stopping it frees its memory and CPU.",
        );
        if self.vms.is_empty() {
            vms = vms.row(
                Row::new("no-vm", "No VM found")
                    .detail("Install Colima, OrbStack, Docker Desktop or Podman."),
            );
        }
        for &(vm, running) in &self.vms {
            let busy = self.busy == Some(vm);
            vms = vms.row(
                Row::new(SharedString::from(format!("vm-{}", vm.key())), vm.name())
                    .detail(if running { "Running" } else { "Stopped" })
                    .control(
                        Button::new(
                            SharedString::from(format!("vm-toggle-{}", vm.key())),
                            if running { "Stop" } else { "Start" },
                        )
                        .style(ButtonStyle::Outlined)
                        .size(ButtonSize::Compact)
                        .disabled(busy || self.busy.is_some())
                        .on_click(cx.listener(move |view, _, _, cx| view.toggle(vm, running, cx))),
                    ),
            );
        }
        let mut options: Vec<&'static str> = vec!["Pick automatically"];
        options.extend(self.vms.iter().map(|(vm, _)| vm.name()));
        let chosen = settings
            .vm
            .and_then(|vm| self.vms.iter().position(|(v, _)| *v == vm))
            .map_or(0, |i| i + 1);
        let installed: Vec<Vm> = self.vms.iter().map(|(vm, _)| *vm).collect();
        vms = vms
            .row(
                Row::new("fec-vm", "VM for fec").detail(
                    "Which one `fec start`, `stop` and docker commands use. Automatic: the running one.",
                )
                .control(select(
                    "fec-vm-select",
                    &options,
                    chosen,
                    move |i, _, cx| {
                        let vm = i.checked_sub(1).and_then(|i| installed.get(i).copied());
                        update(|s| s.vm = vm);
                        cx.refresh_windows();
                    },
                    window,
                    cx,
                )),
            )
            .when_some(self.message.clone(), |section, message| section.footnote(message));

        let stop_index = AUTO_STOP
            .iter()
            .position(|m| *m == settings.auto_stop_minutes)
            .unwrap_or(3);
        let idle = Section::new("When idle").row(
            Row::new("auto-stop", "Stop the VM when no container runs for")
                .detail("Checked while FerrumContainers is open. Start it again from the app or with `fec start`.")
                .control(select(
                    "auto-stop-select",
                    &AUTO_STOP_LABELS,
                    stop_index,
                    |i, _, cx| {
                        update(|s| s.auto_stop_minutes = AUTO_STOP[i]);
                        cx.refresh_windows();
                    },
                    window,
                    cx,
                )),
        );

        let mut cli = Section::new("Command line: fec").detail(
            "Like colima and docker in one. Turn the `fec` command on in FerrumLauncher if your shell can't find it.",
        );
        for (i, (command, what)) in COMMANDS.iter().enumerate() {
            cli = cli.row(
                Row::new(("fec-command", i), *what).control(
                    div()
                        .px_1p5()
                        .rounded_sm()
                        .bg(colors.element_background)
                        .text_color(colors.text)
                        .text_size(px(12.))
                        .font_family(ferrum_ui::assets::BUFFER_FONT)
                        .child(*command),
                ),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(vms)
            .child(idle)
            .child(cli)
    }
}
