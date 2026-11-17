//! Kubernetes, read-only plus one action: contexts and namespaces of the
//! current kubeconfig, pods, deployments, services, pod logs, and
//! `rollout restart` of a deployment (the UI confirms first).
//!
//! There is no Kubernetes client crate in the workspace (FerrumSystem's live
//! pods also use `kubectl`), and a full client is a heavy dependency, so
//! this runs `kubectl … -o json` and reuses FerrumSystem's pod parser.
//! `kubectl` brings every kubeconfig auth plugin (gke, aws, oidc) for free.

use crate::error::{EngineError, EngineResult};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub use ferrumsystem_core::runtime::Pod;

/// Each call gives up after this (`--request-timeout`).
const REQUEST_TIMEOUT: &str = "--request-timeout=10s";
/// Lines of a pod's log shown.
pub const LOG_TAIL: u32 = 1_000;

/// Install locations checked after `PATH` (relative to `$HOME` when not absolute).
const FALLBACKS: [&str; 6] = [
    "/usr/local/bin/kubectl",
    "/opt/homebrew/bin/kubectl",
    ".orbstack/bin/kubectl",
    ".rd/bin/kubectl",
    ".docker/bin/kubectl",
    "/Applications/Docker.app/Contents/Resources/bin/kubectl",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kubectl {
    pub path: PathBuf,
}

/// Finds `kubectl` on `path_env`, then in the fallback locations.
#[must_use]
pub fn find_kubectl(
    path_env: Option<&std::ffi::OsStr>,
    home: Option<&Path>,
    exists: impl Fn(&Path) -> bool,
) -> Option<Kubectl> {
    path_env
        .into_iter()
        .flat_map(std::env::split_paths)
        .map(|dir| dir.join("kubectl"))
        .chain(FALLBACKS.iter().filter_map(|f| {
            if f.starts_with('/') {
                Some(PathBuf::from(f))
            } else {
                home.map(|h| h.join(f))
            }
        }))
        .find(|p| exists(p))
        .map(|path| Kubectl { path })
}

impl Kubectl {
    #[must_use]
    pub fn detect() -> Option<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        find_kubectl(
            std::env::var_os("PATH").as_deref(),
            home.as_deref(),
            Path::is_file,
        )
    }

    /// Runs `kubectl <args>`; stdout, or stderr as the error.
    pub async fn run(&self, args: &[String]) -> EngineResult<String> {
        let mut path = std::env::var_os("PATH").unwrap_or_default();
        // Auth plugins (gke-gcloud-auth-plugin, aws…) live next to the usual tools.
        if let Some(dir) = self.path.parent() {
            let mut dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
            for extra in [
                dir,
                Path::new("/usr/local/bin"),
                Path::new("/opt/homebrew/bin"),
            ] {
                if !dirs.iter().any(|d| d == extra) {
                    dirs.push(extra.to_path_buf());
                }
            }
            path = std::env::join_paths(dirs).unwrap_or(path);
        }
        let output = tokio::process::Command::new(&self.path)
            .args(args)
            .env("PATH", path)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .map_err(|e| {
                EngineError::Command(format!("could not run {}: {e}", self.path.display()))
            })?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            Err(EngineError::Command(if error.is_empty() {
                format!("kubectl exited with {}", output.status)
            } else {
                error
            }))
        }
    }

    pub async fn config(&self) -> EngineResult<KubeConfig> {
        let json = self.run(&config_args()).await?;
        parse_config(&json)
    }

    pub async fn namespaces(&self, context: &str) -> EngineResult<Vec<String>> {
        let json = self.run(&get_args(context, None, "namespaces")).await?;
        Ok(items(&json)?
            .iter()
            .filter_map(|i| i["metadata"]["name"].as_str().map(str::to_owned))
            .collect())
    }

    pub async fn pods(&self, context: &str, namespace: &str) -> EngineResult<Vec<Pod>> {
        let json = self
            .run(&get_args(context, Some(namespace), "pods"))
            .await?;
        ferrumsystem_core::runtime::parse_pods(&json).map_err(EngineError::Command)
    }

    pub async fn deployments(
        &self,
        context: &str,
        namespace: &str,
    ) -> EngineResult<Vec<Deployment>> {
        let json = self
            .run(&get_args(context, Some(namespace), "deployments"))
            .await?;
        parse_deployments(&json)
    }

    pub async fn services(&self, context: &str, namespace: &str) -> EngineResult<Vec<Service>> {
        let json = self
            .run(&get_args(context, Some(namespace), "services"))
            .await?;
        parse_services(&json)
    }

    pub async fn logs(&self, context: &str, namespace: &str, pod: &str) -> EngineResult<String> {
        self.run(&logs_args(context, namespace, pod)).await
    }

    /// `kubectl rollout restart deployment/<name>`: new pods replace the old ones.
    pub async fn restart_deployment(
        &self,
        context: &str,
        namespace: &str,
        name: &str,
    ) -> EngineResult<String> {
        tracing::info!(%context, %namespace, deployment = %name, "kubectl rollout restart");
        self.run(&restart_args(context, namespace, name)).await
    }
}

/// Contexts of the kubeconfig and the current one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KubeConfig {
    pub current: Option<String>,
    pub contexts: Vec<KubeContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubeContext {
    pub name: String,
    pub cluster: String,
    /// The context's default namespace, if set.
    pub namespace: Option<String>,
}

impl KubeConfig {
    /// The namespace to start in for `context`: its own, else `default`.
    #[must_use]
    pub fn namespace_of(&self, context: &str) -> String {
        self.contexts
            .iter()
            .find(|c| c.name == context)
            .and_then(|c| c.namespace.clone())
            .unwrap_or_else(|| "default".to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    pub name: String,
    /// Ready / wanted replicas.
    pub ready: (u64, u64),
    pub up_to_date: u64,
    pub available: u64,
    pub images: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub name: String,
    /// `ClusterIP`, `NodePort`, `LoadBalancer`…
    pub kind: String,
    pub cluster_ip: String,
    /// `80/TCP`, `80:30080/TCP`…
    pub ports: Vec<String>,
    /// Load balancer addresses.
    pub external: Vec<String>,
}

/// `config view -o json`: names only (secrets are redacted by kubectl).
#[must_use]
pub fn config_args() -> Vec<String> {
    ["config", "view", "-o", "json"].map(str::to_owned).to_vec()
}

/// `get <kind> -o json` in `context` (and `namespace`).
#[must_use]
pub fn get_args(context: &str, namespace: Option<&str>, kind: &str) -> Vec<String> {
    let mut args = vec!["--context".to_owned(), context.to_owned()];
    if let Some(ns) = namespace {
        args.extend(["-n".to_owned(), ns.to_owned()]);
    }
    args.extend(["get", kind, "-o", "json", REQUEST_TIMEOUT].map(str::to_owned));
    args
}

#[must_use]
pub fn logs_args(context: &str, namespace: &str, pod: &str) -> Vec<String> {
    vec![
        "--context".into(),
        context.into(),
        "-n".into(),
        namespace.into(),
        "logs".into(),
        pod.into(),
        "--all-containers".into(),
        "--prefix".into(),
        format!("--tail={LOG_TAIL}"),
        REQUEST_TIMEOUT.into(),
    ]
}

#[must_use]
pub fn restart_args(context: &str, namespace: &str, name: &str) -> Vec<String> {
    vec![
        "--context".into(),
        context.into(),
        "-n".into(),
        namespace.into(),
        "rollout".into(),
        "restart".into(),
        format!("deployment/{name}"),
        REQUEST_TIMEOUT.into(),
    ]
}

fn invalid(e: impl std::fmt::Display) -> EngineError {
    EngineError::Command(format!("unexpected kubectl output: {e}"))
}

fn items(json: &str) -> EngineResult<Vec<Value>> {
    let root: Value = serde_json::from_str(json).map_err(invalid)?;
    Ok(root["items"].as_array().cloned().unwrap_or_default())
}

pub fn parse_config(json: &str) -> EngineResult<KubeConfig> {
    let root: Value = serde_json::from_str(json).map_err(invalid)?;
    let contexts = root["contexts"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|c| {
                    Some(KubeContext {
                        name: c["name"].as_str()?.to_owned(),
                        cluster: c["context"]["cluster"].as_str().unwrap_or("").to_owned(),
                        namespace: c["context"]["namespace"]
                            .as_str()
                            .filter(|n| !n.is_empty())
                            .map(str::to_owned),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(KubeConfig {
        current: root["current-context"]
            .as_str()
            .filter(|c| !c.is_empty())
            .map(str::to_owned),
        contexts,
    })
}

pub fn parse_deployments(json: &str) -> EngineResult<Vec<Deployment>> {
    let count = |v: &Value| v.as_u64().unwrap_or(0);
    Ok(items(json)?
        .iter()
        .map(|d| Deployment {
            name: d["metadata"]["name"].as_str().unwrap_or("?").to_owned(),
            ready: (
                count(&d["status"]["readyReplicas"]),
                d["spec"]["replicas"].as_u64().unwrap_or(1),
            ),
            up_to_date: count(&d["status"]["updatedReplicas"]),
            available: count(&d["status"]["availableReplicas"]),
            images: d["spec"]["template"]["spec"]["containers"]
                .as_array()
                .map(|cs| {
                    cs.iter()
                        .filter_map(|c| c["image"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect())
}

pub fn parse_services(json: &str) -> EngineResult<Vec<Service>> {
    Ok(items(json)?
        .iter()
        .map(|s| {
            let ports = s["spec"]["ports"]
                .as_array()
                .map(|ports| {
                    ports
                        .iter()
                        .map(|p| {
                            let port = p["port"].as_u64().unwrap_or(0);
                            let protocol = p["protocol"].as_str().unwrap_or("TCP");
                            match p["nodePort"].as_u64() {
                                Some(node) => format!("{port}:{node}/{protocol}"),
                                None => format!("{port}/{protocol}"),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let external = s["status"]["loadBalancer"]["ingress"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|i| i["ip"].as_str().or_else(|| i["hostname"].as_str()))
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            Service {
                name: s["metadata"]["name"].as_str().unwrap_or("?").to_owned(),
                kind: s["spec"]["type"].as_str().unwrap_or("ClusterIP").to_owned(),
                cluster_ip: s["spec"]["clusterIP"].as_str().unwrap_or("").to_owned(),
                ports,
                external,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_kubectl() {
        let path = std::env::join_paths(["/nope", "/tools"]).unwrap_or_default();
        let found = find_kubectl(Some(&path), None, |p| p == Path::new("/tools/kubectl"));
        assert_eq!(found.map(|k| k.path), Some(PathBuf::from("/tools/kubectl")));
        let found = find_kubectl(None, Some(Path::new("/Users/me")), |p| {
            p == Path::new("/Users/me/.orbstack/bin/kubectl")
        });
        assert!(found.is_some());
        assert_eq!(find_kubectl(None, None, |_| false), None);
    }

    #[test]
    fn builds_kubectl_arguments() {
        assert_eq!(
            get_args("kind-dev", Some("shop"), "pods").join(" "),
            "--context kind-dev -n shop get pods -o json --request-timeout=10s"
        );
        assert_eq!(
            get_args("kind-dev", None, "namespaces").join(" "),
            "--context kind-dev get namespaces -o json --request-timeout=10s"
        );
        assert_eq!(
            restart_args("kind-dev", "shop", "api").join(" "),
            "--context kind-dev -n shop rollout restart deployment/api --request-timeout=10s"
        );
        assert!(logs_args("c", "n", "api-1").contains(&"--tail=1000".to_owned()));
    }

    #[test]
    fn reads_the_kubeconfig() {
        let json = r#"{"current-context":"kind-dev","contexts":[
            {"name":"kind-dev","context":{"cluster":"kind-dev","namespace":"shop"}},
            {"name":"prod","context":{"cluster":"gke-prod","user":"me"}}]}"#;
        let config = parse_config(json).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(config.current.as_deref(), Some("kind-dev"));
        assert_eq!(config.contexts.len(), 2);
        assert_eq!(config.namespace_of("kind-dev"), "shop");
        assert_eq!(config.namespace_of("prod"), "default");
        assert_eq!(parse_config("{}").map(|c| c.contexts.len()), Ok(0));
        assert!(parse_config("nope").is_err());
    }

    #[test]
    fn reads_deployments_and_services() {
        let deployments = parse_deployments(
            r#"{"items":[{"metadata":{"name":"api"},
                "spec":{"replicas":3,"template":{"spec":{"containers":[{"image":"shop/api:1.4"}]}}},
                "status":{"readyReplicas":2,"updatedReplicas":3,"availableReplicas":2}},
                {"metadata":{"name":"idle"},"spec":{"replicas":0},"status":{}}]}"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(deployments[0].ready, (2, 3));
        assert_eq!(deployments[0].images, ["shop/api:1.4"]);
        assert_eq!(deployments[1].ready, (0, 0));

        let services = parse_services(
            r#"{"items":[{"metadata":{"name":"web"},"spec":{"type":"LoadBalancer","clusterIP":"10.0.0.7",
                "ports":[{"port":80,"nodePort":30080,"protocol":"TCP"},{"port":53,"protocol":"UDP"}]},
                "status":{"loadBalancer":{"ingress":[{"ip":"34.1.2.3"},{"hostname":"lb.example.com"}]}}}]}"#,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(services[0].ports, ["80:30080/TCP", "53/UDP"]);
        assert_eq!(services[0].external, ["34.1.2.3", "lb.example.com"]);
        assert_eq!(services[0].kind, "LoadBalancer");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_kubectl_and_reports_its_errors() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let path = dir.path().join("kubectl");
        let script = r#"#!/bin/sh
case "$*" in
  *"get pods"*) echo '{"items":[{"metadata":{"name":"api-1","labels":{"app":"api"}},"status":{"phase":"Running","containerStatuses":[{"ready":true,"restartCount":1}]}}]}' ;;
  *"rollout restart"*) echo "deployment.apps/api restarted" ;;
  *) echo 'error: context "gone" does not exist' >&2; exit 1 ;;
esac
"#;
        std::fs::write(&path, script).unwrap_or_else(|e| panic!("{e}"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|e| panic!("{e}"));
        let kubectl = Kubectl { path };
        let pods = kubectl.pods("dev", "shop").await;
        let pods = pods.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(pods[0].name, "api-1");
        assert_eq!(pods[0].ready, (1, 1));
        let restarted = kubectl.restart_deployment("dev", "shop", "api").await;
        assert_eq!(
            restarted.ok().as_deref(),
            Some("deployment.apps/api restarted\n")
        );
        let failed = kubectl.namespaces("gone").await;
        assert!(failed.is_err_and(|e| e.to_string().contains("does not exist")));
    }
}
