//! "Run workflow": pick a ref, fill the `workflow_dispatch` inputs, send.

use crate::ci::CiView;
use crate::ci::widgets::select;
use chrono::Utc;
use ferrum_ui::prelude::*;
use ferrumci_core::Workflow;
use ferrumci_core::dispatch::{DispatchSpec, InputKind};
use std::collections::{BTreeMap, HashMap};

pub struct DispatchForm {
    pub workflow: Workflow,
    pub refs: Vec<String>,
    pub git_ref: String,
    /// `None` while loading; `Some(None)` when the workflow has no
    /// `workflow_dispatch` trigger at this ref.
    pub spec: Option<Option<DispatchSpec>>,
    pub values: BTreeMap<String, String>,
    /// Text fields for string / number / environment inputs (made on first render).
    pub fields: HashMap<String, Entity<TextInput>>,
    pub error: Option<String>,
    pub sending: bool,
}

impl CiView {
    pub(crate) fn open_dispatch(&mut self, workflow_id: u64, cx: &mut Context<Self>) {
        let Some(workflow) = self.workflows.iter().find(|w| w.id == workflow_id).cloned() else {
            return;
        };
        let git_ref = self
            .filter
            .branch
            .clone()
            .or_else(|| self.local_branch.clone())
            .unwrap_or_default();
        self.dispatch = Some(DispatchForm {
            workflow,
            refs: Vec::new(),
            git_ref,
            spec: None,
            values: BTreeMap::new(),
            fields: HashMap::new(),
            error: None,
            sending: false,
        });
        self.spawn_forge(
            cx,
            |c| async move {
                let mut branches = c.actions.branches().await.unwrap_or_default();
                let default = c.actions.default_branch().await.unwrap_or_default();
                if !default.is_empty() {
                    branches.retain(|b| *b != default);
                    branches.insert(0, default.clone());
                }
                Ok((branches, default))
            },
            |this, result: ferrumci_core::ForgeResult<(Vec<String>, String)>, cx| {
                let Some(form) = this.dispatch.as_mut() else {
                    return;
                };
                if let Ok((refs, default)) = result {
                    // A local branch that was never pushed cannot be run.
                    let unknown = !refs.is_empty() && !refs.contains(&form.git_ref);
                    if (form.git_ref.is_empty() || unknown) && !default.is_empty() {
                        form.git_ref = default;
                    }
                    form.refs = refs;
                }
                this.load_dispatch_spec(cx);
            },
        );
        cx.notify();
    }

    fn load_dispatch_spec(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.dispatch.as_mut() else {
            return;
        };
        form.spec = None;
        form.fields.clear();
        let (workflow, git_ref) = (form.workflow.clone(), form.git_ref.clone());
        self.spawn_forge(
            cx,
            move |c| async move { c.actions.dispatch_spec(&workflow, &git_ref).await },
            |this, result, _| {
                let Some(form) = this.dispatch.as_mut() else {
                    return;
                };
                match result {
                    Ok(spec) => {
                        form.values = spec
                            .as_ref()
                            .map(DispatchSpec::defaults)
                            .unwrap_or_default();
                        form.spec = Some(spec);
                        form.error = None;
                    }
                    Err(e) => {
                        form.spec = Some(None);
                        form.error = Some(e.to_string());
                    }
                }
            },
        );
    }

    pub(crate) fn set_dispatch_ref(&mut self, git_ref: String, cx: &mut Context<Self>) {
        if let Some(form) = self.dispatch.as_mut() {
            form.git_ref = git_ref;
            self.load_dispatch_spec(cx);
        }
    }

    pub(crate) fn submit_dispatch(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.dispatch.as_mut() else {
            return;
        };
        let Some(Some(spec)) = form.spec.clone() else {
            return;
        };
        for (name, field) in &form.fields {
            form.values
                .insert(name.clone(), field.read(cx).text().to_owned());
        }
        let inputs = match spec.validate(&form.values) {
            Ok(inputs) => inputs,
            Err(e) => {
                form.error = Some(e);
                cx.notify();
                return;
            }
        };
        form.sending = true;
        form.error = None;
        let (id, name, git_ref) = (
            form.workflow.id,
            form.workflow.name.clone(),
            form.git_ref.clone(),
        );
        let r = git_ref.clone();
        self.spawn_forge(
            cx,
            move |c| async move { c.actions.dispatch(id, &r, &inputs).await },
            move |this, result, cx| match result {
                Ok(()) => {
                    this.dispatch = None;
                    this.watcher.expect_dispatch(id, &git_ref, Utc::now());
                    this.refresh_soon();
                    this.show_notice(Notice::info(format!("Started {name} on {git_ref}.")), cx);
                }
                Err(e) => {
                    if let Some(form) = this.dispatch.as_mut() {
                        form.sending = false;
                        form.error = Some(e.to_string());
                    }
                }
            },
        );
        cx.notify();
    }
}

fn ensure_fields(app: &mut CiView, window: &mut Window, cx: &mut Context<CiView>) {
    let Some(form) = app.dispatch.as_mut() else {
        return;
    };
    let Some(Some(spec)) = &form.spec else {
        return;
    };
    for input in &spec.inputs {
        let typed = matches!(
            input.kind,
            InputKind::String | InputKind::Number | InputKind::Environment
        );
        if typed && !form.fields.contains_key(&input.name) {
            let text = form.values.get(&input.name).cloned().unwrap_or_default();
            let field = cx.new(|cx| TextInput::new(window, cx).with_text(text));
            form.fields.insert(input.name.clone(), field);
        }
    }
}

pub fn render(app: &mut CiView, window: &mut Window, cx: &mut Context<CiView>) -> AnyElement {
    ensure_fields(app, window, cx);
    let Some(form) = app.dispatch.as_ref() else {
        return div().into_any_element();
    };
    let ref_options: Vec<(SharedString, String)> = form
        .refs
        .iter()
        .map(|r| (SharedString::from(r.clone()), r.clone()))
        .collect();
    let ref_label = if form.git_ref.is_empty() {
        "Choose a branch".to_owned()
    } else {
        form.git_ref.clone()
    };
    let mut body = v_flex().gap_2().child(
        h_flex()
            .gap_2()
            .child(Label::new("Use workflow from"))
            .child(select(
                "dispatch-ref",
                ref_label,
                ref_options,
                |this, r, cx| this.set_dispatch_ref(r, cx),
                window,
                cx,
            )),
    );
    match &form.spec {
        None => body = body.child(Label::new("Reading the workflow file…").color(Color::Muted)),
        Some(None) if form.error.is_none() => {
            body = body.child(
                Label::new(format!(
                    "{} has no workflow_dispatch trigger on this branch, so it cannot be started by hand.",
                    form.workflow.path
                ))
                .color(Color::Muted),
            );
        }
        Some(None) => {}
        Some(Some(spec)) => {
            if spec.inputs.is_empty() {
                body = body.child(Label::new("This workflow has no inputs.").color(Color::Muted));
            }
            for input in &spec.inputs {
                let name = input.name.clone();
                let title = if input.required {
                    format!("{name} *")
                } else {
                    name.clone()
                };
                let control: AnyElement = match input.kind {
                    InputKind::Boolean | InputKind::Choice => {
                        let options: Vec<String> = if input.kind == InputKind::Boolean {
                            vec!["true".into(), "false".into()]
                        } else {
                            input.options.clone()
                        };
                        let current = form.values.get(&name).cloned().unwrap_or_default();
                        let key = name.clone();
                        select(
                            SharedString::from(format!("dispatch-input-{name}")),
                            if current.is_empty() {
                                "Choose…".to_owned()
                            } else {
                                current
                            },
                            options
                                .into_iter()
                                .map(|o| (SharedString::from(o.clone()), o))
                                .collect(),
                            move |this, v, _| {
                                if let Some(f) = this.dispatch.as_mut() {
                                    f.values.insert(key.clone(), v);
                                }
                            },
                            window,
                            cx,
                        )
                        .into_any_element()
                    }
                    _ => form.fields.get(&name).map_or_else(
                        || div().into_any_element(),
                        |f| f.clone().into_any_element(),
                    ),
                };
                body = body.child(
                    v_flex()
                        .gap_0p5()
                        .child(Label::new(title).size(LabelSize::Small))
                        .child(control)
                        .when(!input.description.is_empty(), |el| {
                            el.child(
                                Label::new(input.description.clone())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                        }),
                );
            }
        }
    }
    if let Some(e) = &form.error {
        body = body.child(
            Label::new(e.clone())
                .size(LabelSize::Small)
                .color(Color::Error),
        );
    }
    let can_send = matches!(form.spec, Some(Some(_))) && !form.sending && !form.git_ref.is_empty();
    Modal::new(format!("Run {}", form.workflow.name))
        .width(px(520.))
        .on_dismiss(cx.listener(|this, _, _, cx| {
            this.dispatch = None;
            cx.notify();
        }))
        .footer(
            Button::new(
                "dispatch-send",
                if form.sending {
                    "Starting…"
                } else {
                    "Run workflow"
                },
            )
            .style(ButtonStyle::Filled)
            .disabled(!can_send)
            .on_click(cx.listener(|this, _, _, cx| this.submit_dispatch(cx))),
        )
        .child(body)
        .into_any_element()
}
