//! What is running: Kubernetes pods (`kubectl get pods -o json`) and
//! Docker containers (`docker ps -a`), matched to the diagram's components
//! by the `app` label or the container name.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::model::{Cluster, Node};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pod {
    pub name: String,
    /// The `app` (or `app.kubernetes.io/name`) label.
    pub app: Option<String>,
    /// `Running`, `Pending`, `CrashLoopBackOff`…
    pub status: String,
    /// Ready containers / all containers.
    pub ready: (u32, u32),
    pub restarts: u32,
    pub node: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    pub name: String,
    pub image: String,
    /// `running`, `exited`…
    pub state: String,
    /// `Up 2 hours`…
    pub status: String,
}

/// Runs `kubectl` (at `exe`) for `cluster`'s namespace.
pub fn pods(exe: &Path, cluster: &Cluster) -> Result<Vec<Pod>, String> {
    let mut cmd = Command::new(exe);
    if let Some(context) = &cluster.context {
        cmd.args(["--context", context]);
    }
    let namespace = if cluster.namespace.is_empty() {
        "default"
    } else {
        &cluster.namespace
    };
    cmd.args([
        "get",
        "pods",
        "-n",
        namespace,
        "-o",
        "json",
        "--request-timeout=8s",
    ]);
    parse_pods(&run(cmd)?)
}

/// Runs `docker ps -a` (at `exe`).
pub fn containers(exe: &Path) -> Result<Vec<Container>, String> {
    let mut cmd = Command::new(exe);
    cmd.args(["ps", "-a", "--format", "{{json .}}"]);
    Ok(parse_containers(&run(cmd)?))
}

fn run(mut cmd: Command) -> Result<String, String> {
    let out = cmd.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(if err.is_empty() {
            format!("exited with {}", out.status)
        } else {
            err
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn parse_pods(json: &str) -> Result<Vec<Pod>, String> {
    let root: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let items = root["items"].as_array().cloned().unwrap_or_default();
    Ok(items
        .iter()
        .map(|item| {
            let labels = &item["metadata"]["labels"];
            let app = labels["app"]
                .as_str()
                .or_else(|| labels["app.kubernetes.io/name"].as_str())
                .map(str::to_owned);
            let statuses = item["status"]["containerStatuses"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let ready = statuses.iter().filter(|s| s["ready"] == true).count() as u32;
            let restarts = statuses
                .iter()
                .filter_map(|s| s["restartCount"].as_u64())
                .sum::<u64>() as u32;
            // A waiting reason (CrashLoopBackOff…) says more than the phase.
            let waiting = statuses
                .iter()
                .find_map(|s| s["state"]["waiting"]["reason"].as_str());
            let status = waiting
                .or_else(|| item["status"]["phase"].as_str())
                .unwrap_or("Unknown")
                .to_owned();
            Pod {
                name: item["metadata"]["name"].as_str().unwrap_or("?").to_owned(),
                app,
                status,
                ready: (ready, statuses.len() as u32),
                restarts,
                node: item["spec"]["nodeName"].as_str().map(str::to_owned),
            }
        })
        .collect())
}

pub fn parse_containers(lines: &str) -> Vec<Container> {
    lines
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|v| Container {
            name: v["Names"].as_str().unwrap_or("?").to_owned(),
            image: v["Image"].as_str().unwrap_or("").to_owned(),
            state: v["State"].as_str().unwrap_or("").to_owned(),
            status: v["Status"].as_str().unwrap_or("").to_owned(),
        })
        .collect()
}

/// What `node` matches on: its deploy selector, else its id.
pub fn selector(node: &Node) -> &str {
    node.deploy
        .as_ref()
        .and_then(|d| d.selector.as_deref())
        .unwrap_or(&node.id)
}

/// The pods of `node`.
pub fn pods_of<'a>(node: &Node, pods: &'a [Pod]) -> Vec<&'a Pod> {
    let sel = selector(node);
    pods.iter()
        .filter(|p| p.app.as_deref() == Some(sel))
        .collect()
}

/// The containers of `node` (name equal to, or containing, its selector).
pub fn containers_of<'a>(node: &Node, containers: &'a [Container]) -> Vec<&'a Container> {
    let sel = selector(node);
    containers
        .iter()
        .filter(|c| c.name == sel || c.name.contains(sel))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pods_and_containers() {
        let json = r#"{"items":[{"metadata":{"name":"ledger-7d9","labels":{"app":"ledger"}},
            "spec":{"nodeName":"worker-1"},
            "status":{"phase":"Running","containerStatuses":[{"ready":true,"restartCount":2,"state":{"running":{}}}]}},
            {"metadata":{"name":"fraud-1","labels":{"app.kubernetes.io/name":"fraud"}},
            "status":{"phase":"Running","containerStatuses":[{"ready":false,"restartCount":5,
            "state":{"waiting":{"reason":"CrashLoopBackOff"}}}]}}]}"#;
        let pods = parse_pods(json).expect("pods");
        assert_eq!(pods[0].ready, (1, 1));
        assert_eq!(pods[0].restarts, 2);
        assert_eq!(pods[1].status, "CrashLoopBackOff");
        assert_eq!(pods[1].app.as_deref(), Some("fraud"));

        let node = Node {
            id: "ledger".into(),
            ..Node::default()
        };
        assert_eq!(pods_of(&node, &pods).len(), 1);

        let lines = "{\"Names\":\"bank-ledger-1\",\"Image\":\"ledger:1\",\"State\":\"running\",\"Status\":\"Up 2 hours\"}\nnot json\n";
        let containers = parse_containers(lines);
        assert_eq!(containers.len(), 1);
        assert_eq!(containers_of(&node, &containers).len(), 1);
    }
}
