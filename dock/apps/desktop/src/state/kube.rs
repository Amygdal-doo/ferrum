//! Kubernetes section state: the kubeconfig's contexts, the context and
//! namespace shown, and the lists fetched with `kubectl`.

use ferrumdock_engine::kube::{Deployment, KubeConfig, Kubectl, Pod, Service};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KubeTab {
    #[default]
    Pods,
    Deployments,
    Services,
}

impl KubeTab {
    pub const ALL: [Self; 3] = [Self::Pods, Self::Deployments, Self::Services];

    pub fn label(self) -> &'static str {
        match self {
            Self::Pods => "Pods",
            Self::Deployments => "Deployments",
            Self::Services => "Services",
        }
    }
}

/// What the picker dialog chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KubePicker {
    Context,
    Namespace,
}

#[derive(Default)]
pub struct KubeState {
    /// Looked for once, when the section is first opened.
    pub kubectl: Option<Kubectl>,
    pub checked: bool,
    pub config: Option<Result<KubeConfig, String>>,
    pub context: Option<String>,
    pub namespace: String,
    pub namespaces: Option<Result<Vec<String>, String>>,
    pub tab: KubeTab,
    pub pods: Option<Result<Vec<Pod>, String>>,
    pub deployments: Option<Result<Vec<Deployment>, String>>,
    pub services: Option<Result<Vec<Service>, String>>,
    pub loading: bool,
    /// The pod whose logs are shown, and what was read.
    pub logs: Option<(String, Option<Result<String, String>>)>,
}

impl KubeState {
    /// Switches to `context` in its default namespace and forgets the lists.
    pub fn use_context(&mut self, context: String) {
        self.namespace = match &self.config {
            Some(Ok(config)) => config.namespace_of(&context),
            _ => "default".to_owned(),
        };
        self.context = Some(context);
        self.namespaces = None;
        self.clear_lists();
    }

    pub fn clear_lists(&mut self) {
        self.pods = None;
        self.deployments = None;
        self.services = None;
    }

    /// Rows in the current tab, once loaded.
    pub fn count(&self) -> Option<usize> {
        match self.tab {
            KubeTab::Pods => self.pods.as_ref()?.as_ref().ok().map(Vec::len),
            KubeTab::Deployments => self.deployments.as_ref()?.as_ref().ok().map(Vec::len),
            KubeTab::Services => self.services.as_ref()?.as_ref().ok().map(Vec::len),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdock_engine::kube::KubeContext;

    #[test]
    fn switching_context_uses_its_namespace_and_clears_lists() {
        let mut state = KubeState {
            config: Some(Ok(KubeConfig {
                current: Some("dev".into()),
                contexts: vec![KubeContext {
                    name: "dev".into(),
                    cluster: "kind".into(),
                    namespace: Some("shop".into()),
                }],
            })),
            pods: Some(Ok(Vec::new())),
            ..KubeState::default()
        };
        assert_eq!(state.count(), Some(0));
        state.use_context("dev".into());
        assert_eq!(state.namespace, "shop");
        assert!(state.pods.is_none() && state.count().is_none());
        state.use_context("other".into());
        assert_eq!(state.namespace, "default");
    }
}
