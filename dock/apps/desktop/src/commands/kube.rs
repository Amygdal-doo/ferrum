//! The Kubernetes section: `kubectl` lookups for the context and namespace
//! shown, pod logs, and `rollout restart` of a deployment after a confirm.

use crate::app::FerrumContainers;
use crate::state::{Confirm, Dialog, KubePicker, KubeTab, PendingAction};
use ferrumdock_engine::kube::{Kubectl, LOG_TAIL};
use gpui::Context;

impl FerrumContainers {
    /// `kubectl`, the context and the namespace, when all are known.
    fn kube_target(&self) -> Option<(Kubectl, String, String)> {
        let kube = &self.state.kube;
        Some((
            kube.kubectl.clone()?,
            kube.context.clone()?,
            kube.namespace.clone(),
        ))
    }

    /// Loads what the section shows: the kubeconfig first (once), then the
    /// current tab's list.
    pub fn load_kube(&mut self, cx: &mut Context<Self>) {
        let kube = &mut self.state.kube;
        if !kube.checked {
            kube.checked = true;
            kube.kubectl = Kubectl::detect();
            match &kube.kubectl {
                Some(k) => tracing::info!(path = %k.path.display(), "kubectl"),
                None => tracing::info!("kubectl not found: the Kubernetes section explains"),
            }
        }
        let Some(kubectl) = kube.kubectl.clone() else {
            cx.notify();
            return;
        };
        if kube.config.as_ref().is_none_or(Result::is_err) {
            kube.loading = true;
            self.spawn_run(
                cx,
                async move { kubectl.config().await },
                |app, result, cx| {
                    app.state.kube.loading = false;
                    let first = result.as_ref().ok().and_then(|c| {
                        c.current
                            .clone()
                            .or_else(|| c.contexts.first().map(|k| k.name.clone()))
                    });
                    app.state.kube.config = Some(result);
                    if let Some(context) = first {
                        app.state.kube.use_context(context);
                        app.load_kube_list(cx);
                    }
                },
            );
            cx.notify();
            return;
        }
        self.load_kube_list(cx);
    }

    /// Fetches the current tab's list for the context and namespace shown.
    pub fn load_kube_list(&mut self, cx: &mut Context<Self>) {
        let Some((kubectl, context, namespace)) = self.kube_target() else {
            return;
        };
        let tab = self.state.kube.tab;
        self.state.kube.loading = true;
        let key = (context.clone(), namespace.clone(), tab);
        self.spawn_run(
            cx,
            async move {
                Ok::<_, String>(match tab {
                    KubeTab::Pods => KubeList::Pods(kubectl.pods(&context, &namespace).await),
                    KubeTab::Deployments => {
                        KubeList::Deployments(kubectl.deployments(&context, &namespace).await)
                    }
                    KubeTab::Services => {
                        KubeList::Services(kubectl.services(&context, &namespace).await)
                    }
                })
            },
            move |app, result, _| {
                let kube = &mut app.state.kube;
                // A list for a context, namespace or tab no longer shown is dropped.
                if (
                    kube.context.clone().unwrap_or_default(),
                    kube.namespace.clone(),
                    kube.tab,
                ) != key
                {
                    return;
                }
                kube.loading = false;
                let text = |e: ferrumdock_engine::EngineError| e.to_string();
                match result {
                    Ok(KubeList::Pods(r)) => kube.pods = Some(r.map_err(text)),
                    Ok(KubeList::Deployments(r)) => kube.deployments = Some(r.map_err(text)),
                    Ok(KubeList::Services(r)) => kube.services = Some(r.map_err(text)),
                    Err(_) => {}
                }
            },
        );
        cx.notify();
    }

    pub fn set_kube_tab(&mut self, tab: KubeTab, cx: &mut Context<Self>) {
        self.state.kube.tab = tab;
        self.load_kube_list(cx);
    }

    /// Opens the context or namespace picker (namespaces are fetched then).
    pub fn open_kube_picker(&mut self, picker: KubePicker, cx: &mut Context<Self>) {
        self.state.dialog = Some(Dialog::Kube(picker));
        if picker == KubePicker::Namespace {
            let Some((kubectl, context, _)) = self.kube_target() else {
                return;
            };
            self.state.kube.namespaces = None;
            self.spawn_run(
                cx,
                async move { kubectl.namespaces(&context).await },
                |app, result, _| app.state.kube.namespaces = Some(result),
            );
        }
        cx.notify();
    }

    pub fn pick_kube_context(&mut self, context: String, cx: &mut Context<Self>) {
        self.state.dialog = None;
        self.state.kube.use_context(context);
        self.load_kube_list(cx);
    }

    pub fn pick_kube_namespace(&mut self, namespace: String, cx: &mut Context<Self>) {
        self.state.dialog = None;
        self.state.kube.namespace = namespace;
        self.state.kube.clear_lists();
        self.load_kube_list(cx);
    }

    /// Shows the last [`LOG_TAIL`] lines of every container of `pod`.
    pub fn open_pod_logs(&mut self, pod: String, cx: &mut Context<Self>) {
        let Some((kubectl, context, namespace)) = self.kube_target() else {
            return;
        };
        self.state.kube.logs = Some((pod.clone(), None));
        self.state.dialog = Some(Dialog::PodLogs);
        self.inputs
            .pod_logs
            .update(cx, |input, cx| input.set_text("", cx));
        let target = pod.clone();
        self.spawn_run(
            cx,
            async move { kubectl.logs(&context, &namespace, &target).await },
            move |app, result, cx| {
                if app.state.kube.logs.as_ref().is_some_and(|(p, _)| *p == pod) {
                    let text = result.as_ref().map_or_else(String::clone, String::clone);
                    app.inputs
                        .pod_logs
                        .update(cx, |input, cx| input.set_text(text, cx));
                    app.state.kube.logs = Some((pod, Some(result)));
                }
            },
        );
        cx.notify();
    }

    /// Asks before `kubectl rollout restart` (it replaces every pod).
    pub fn confirm_restart_deployment(&mut self, name: String, cx: &mut Context<Self>) {
        let Some((_, context, namespace)) = self.kube_target() else {
            return;
        };
        self.ask(
            Confirm {
                title: format!("Restart deployment {name}"),
                body: format!(
                    "kubectl rollout restart deployment/{name} in {namespace} on {context}. \
                     Its pods are replaced one by one, as for a new rollout."
                ),
                confirm_label: "Restart".into(),
                action: PendingAction::RestartDeployment {
                    context,
                    namespace,
                    name,
                },
            },
            cx,
        );
    }

    pub fn restart_deployment(
        &mut self,
        context: String,
        namespace: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(kubectl) = self.state.kube.kubectl.clone() else {
            return;
        };
        let key = format!("deployment:{name}");
        if !self.state.busy.insert(key.clone()) {
            return;
        }
        let target = name.clone();
        self.spawn_run(
            cx,
            async move {
                kubectl
                    .restart_deployment(&context, &namespace, &target)
                    .await
            },
            move |app, result, cx| {
                app.state.busy.remove(&key);
                app.report(&result, format!("Restarting deployment {name}"), cx);
                app.load_kube_list(cx);
            },
        );
        cx.notify();
    }
}

/// One tab's list, as fetched.
enum KubeList {
    Pods(ferrumdock_engine::EngineResult<Vec<ferrumdock_engine::kube::Pod>>),
    Deployments(ferrumdock_engine::EngineResult<Vec<ferrumdock_engine::kube::Deployment>>),
    Services(ferrumdock_engine::EngineResult<Vec<ferrumdock_engine::kube::Service>>),
}

/// The logs dialog's caption.
pub fn logs_caption(pod: &str) -> String {
    format!("{pod} · last {LOG_TAIL} lines of each container")
}
