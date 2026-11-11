//! Add / edit / remove a source.

use crate::logs::LogsView;
use ferrum_ui::gpui::AnyElement;
use ferrum_ui::prelude::*;
use ferrum_ui::ui::Checkbox;
use ferrumlogs_core::{SourceConfig, SourceKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindChoice {
    File,
    Command,
    Docker,
    Compose,
}

impl KindChoice {
    const ALL: [KindChoice; 4] = [
        KindChoice::Command,
        KindChoice::File,
        KindChoice::Docker,
        KindChoice::Compose,
    ];

    fn of(kind: &SourceKind) -> Self {
        match kind {
            SourceKind::File { .. } => KindChoice::File,
            SourceKind::Command { .. } => KindChoice::Command,
            SourceKind::Docker { .. } => KindChoice::Docker,
            SourceKind::Compose { .. } => KindChoice::Compose,
        }
    }

    fn label(self) -> &'static str {
        match self {
            KindChoice::File => "File",
            KindChoice::Command => "Command",
            KindChoice::Docker => "Container",
            KindChoice::Compose => "Compose",
        }
    }

    fn target_label(self) -> &'static str {
        match self {
            KindChoice::File => "Path or glob",
            KindChoice::Command => "Command",
            KindChoice::Docker => "Container name or id",
            KindChoice::Compose => "Services (empty = all)",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            KindChoice::File => "logs/*.log, storage/logs/laravel.log, /var/log/x.log",
            KindChoice::Command => "npm run dev, cargo run, kubectl logs -f deploy/api",
            KindChoice::Docker => "my-app-web-1",
            KindChoice::Compose => "web worker",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            KindChoice::File => {
                "Relative to the project folder. Follows new lines; handles truncation and rotation."
            }
            KindChoice::Command => {
                "Runs in the project folder with your login shell; stdout and stderr are captured."
            }
            KindChoice::Docker => "Follows `docker logs -f --timestamps`.",
            KindChoice::Compose => "Runs `docker compose logs -f` in the project folder.",
        }
    }

    fn build(self, target: String) -> SourceKind {
        match self {
            KindChoice::File => SourceKind::File { path: target },
            KindChoice::Command => SourceKind::Command { command: target },
            KindChoice::Docker => SourceKind::Docker { container: target },
            KindChoice::Compose => SourceKind::Compose { service: target },
        }
    }
}

pub struct SourceDialog {
    /// The source being edited (None: adding one).
    pub editing: Option<SourceConfig>,
    pub kind: KindChoice,
    pub name: Entity<TextInput>,
    pub target: Entity<TextInput>,
    pub autostart: bool,
    pub error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl SourceDialog {
    pub fn new(
        editing: Option<SourceConfig>,
        window: &mut Window,
        cx: &mut Context<LogsView>,
    ) -> Self {
        let kind = editing
            .as_ref()
            .map_or(KindChoice::Command, |c| KindChoice::of(&c.kind));
        let name = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Name (optional)")
                .with_text(editing.as_ref().map(|c| c.name.clone()).unwrap_or_default())
        });
        let target = cx.new(|cx| {
            TextInput::new(window, cx)
                .monospace()
                .placeholder(kind.placeholder())
                .with_text(
                    editing
                        .as_ref()
                        .map(|c| c.kind.target().to_owned())
                        .unwrap_or_default(),
                )
        });
        let submit = |app: &mut LogsView,
                      _: &Entity<TextInput>,
                      event: &TextInputEvent,
                      window: &mut Window,
                      cx: &mut Context<LogsView>| {
            if *event == TextInputEvent::Submit {
                app.submit_dialog(false, window, cx);
            }
        };
        let subscriptions = vec![
            cx.subscribe_in(&name, window, submit),
            cx.subscribe_in(&target, window, submit),
        ];
        target.update(cx, |input, cx| input.focus(window, cx));
        Self {
            autostart: editing.as_ref().is_some_and(|c| c.autostart),
            editing,
            kind,
            name,
            target,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    /// The source described by the form.
    pub fn build(&self, cx: &App) -> Result<SourceConfig, String> {
        let target = self.target.read(cx).text().trim().to_owned();
        if target.is_empty() && self.kind != KindChoice::Compose {
            return Err(format!("{} is required", self.kind.target_label()));
        }
        let mut name = self.name.read(cx).text().trim().to_owned();
        if name.is_empty() {
            name = default_name(self.kind, &target);
        }
        let kind = self.kind.build(target);
        let mut config = match &self.editing {
            Some(existing) => existing.clone(),
            None => SourceConfig::new(name.clone(), kind.clone()),
        };
        config.name = name;
        config.kind = kind;
        config.autostart = self.autostart;
        Ok(config)
    }
}

/// `npm run dev` → `npm run dev`; `storage/logs/laravel.log` → `laravel.log`.
fn default_name(kind: KindChoice, target: &str) -> String {
    match kind {
        KindChoice::File => std::path::Path::new(target)
            .file_name()
            .map_or_else(|| target.to_owned(), |n| n.to_string_lossy().into_owned()),
        KindChoice::Compose if target.is_empty() => "compose".to_owned(),
        _ => target.chars().take(32).collect(),
    }
}

impl LogsView {
    pub fn submit_dialog(&mut self, start: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        match dialog.build(cx) {
            Ok(config) => {
                self.close_dialog(window, cx);
                self.save_source(config, start, cx);
            }
            Err(e) => {
                dialog.error = Some(e);
                cx.notify();
            }
        }
    }

    fn delete_from_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self
            .dialog
            .as_ref()
            .and_then(|d| d.editing.as_ref())
            .map(|c| c.id.clone());
        self.close_dialog(window, cx);
        if let Some(id) = id {
            self.remove_source(&id, cx);
        }
    }
}

pub fn render(app: &LogsView, cx: &mut Context<LogsView>) -> AnyElement {
    let Some(dialog) = app.dialog.as_ref() else {
        return div().into_any_element();
    };
    let current = dialog.kind;
    let mut kinds = h_flex().gap_1();
    for kind in KindChoice::ALL {
        kinds = kinds.child(
            Button::new(
                SharedString::from(format!("kind-{}", kind.label())),
                kind.label(),
            )
            .style(ButtonStyle::Outlined)
            .size(ButtonSize::Compact)
            .toggle_state(kind == current)
            .on_click(cx.listener(move |app, _, _, cx| {
                if let Some(dialog) = app.dialog.as_mut() {
                    dialog.kind = kind;
                    dialog.error = None;
                    dialog.target.update(cx, |input, cx| {
                        input.set_placeholder(kind.placeholder(), cx)
                    });
                }
                cx.notify();
            })),
        );
    }
    let field = |label: &'static str, input: Entity<TextInput>| {
        v_flex()
            .gap_1()
            .child(Label::new(label).size(LabelSize::Small).color(Color::Muted))
            .child(input)
    };
    let editing = dialog.editing.is_some();
    let mut modal = Modal::new(if editing { "Edit source" } else { "Add source" })
        .width(px(560.))
        .on_dismiss(cx.listener(|app, _, window, cx| app.close_dialog(window, cx)))
        .child(kinds)
        .child(field(current.target_label(), dialog.target.clone()))
        .child(
            Label::new(current.hint())
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .child(field("Name", dialog.name.clone()))
        .child(
            Checkbox::new("autostart", ToggleState::from(dialog.autostart))
                .label("Start when the project opens")
                .label_size(LabelSize::Small)
                .on_click(cx.listener(|app, _: &ToggleState, _, cx| {
                    if let Some(dialog) = app.dialog.as_mut() {
                        dialog.autostart = !dialog.autostart;
                    }
                    cx.notify();
                })),
        )
        .when_some(dialog.error.clone(), |modal, error| {
            modal.child(Label::new(error).size(LabelSize::Small).color(Color::Error))
        });
    if editing {
        modal = modal.footer(
            Button::new("delete-source", "Remove")
                .style(ButtonStyle::Subtle)
                .color(Color::Error)
                .on_click(cx.listener(|app, _, window, cx| app.delete_from_dialog(window, cx))),
        );
    }
    modal
        .footer(
            Button::new("cancel-source", "Cancel")
                .style(ButtonStyle::Subtle)
                .on_click(cx.listener(|app, _, window, cx| app.close_dialog(window, cx))),
        )
        .footer(
            Button::new("save-source", "Save")
                .style(ButtonStyle::Outlined)
                .on_click(cx.listener(|app, _, window, cx| app.submit_dialog(false, window, cx))),
        )
        .footer(
            Button::new("save-start-source", "Save & start")
                .style(ButtonStyle::Filled)
                .on_click(cx.listener(|app, _, window, cx| app.submit_dialog(true, window, cx))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_names() {
        assert_eq!(
            default_name(KindChoice::File, "storage/logs/laravel.log"),
            "laravel.log"
        );
        assert_eq!(default_name(KindChoice::Compose, ""), "compose");
        assert_eq!(
            default_name(KindChoice::Command, "npm run dev"),
            "npm run dev"
        );
    }
}
