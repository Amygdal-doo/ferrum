//! Kubernetes manifests: a component per workload (Deployment,
//! StatefulSet, DaemonSet, Job, CronJob) running in pods, one per Ingress
//! in front of the workloads it routes to, and connections from addresses
//! of Services in the containers' environment.

use std::collections::BTreeMap;

use serde::Deserialize as _;
use serde_yaml_ng::Value;

use super::{Builder, guess, mentioned, node};
use crate::model::{Cluster, Deploy, DeployTarget};

type Labels = BTreeMap<String, String>;

struct Workload {
    id: String,
    labels: Labels,
    env: Vec<String>,
}

/// The Kubernetes objects in `text` (one or more YAML documents).
pub(super) fn parse(text: &str) -> Vec<Value> {
    serde_yaml_ng::Deserializer::from_str(text)
        .filter_map(|d| Value::deserialize(d).ok())
        .filter(|d| d.get("apiVersion").is_some() && d.get("kind").is_some())
        .collect()
}

/// Adds what the objects of every manifest (read together: a Service in
/// one file selects a Deployment in another) describe.
pub(super) fn read(docs: &[Value], b: &mut Builder) {
    let mut workloads = Vec::new();
    // Service name → its pod selector.
    let mut services: Vec<(String, Labels)> = Vec::new();
    let mut ingresses: Vec<(String, Vec<String>)> = Vec::new();
    for doc in docs {
        let kind = doc.get("kind").and_then(Value::as_str).unwrap_or_default();
        let name = str_at(doc, &["metadata", "name"]).unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        if b.cluster.is_none()
            && let Some(ns) = str_at(doc, &["metadata", "namespace"])
        {
            b.cluster = Some(Cluster {
                context: None,
                namespace: ns.to_owned(),
            });
        }
        match kind {
            "Deployment" | "StatefulSet" | "DaemonSet" | "Job" | "CronJob" => {
                workloads.push(workload(doc, kind, name, b));
            }
            "Service" => services.push((name.to_owned(), labels(doc.get("spec"), "selector"))),
            "Ingress" => ingresses.push((name.to_owned(), backends(doc))),
            _ => {}
        }
    }
    // A Service's name is the address; it leads to the workload it selects
    // (else, as with manifests in other files, the workload of that name).
    let target = |service: &str| -> String {
        let selector = services.iter().find(|(n, _)| n == service).map(|(_, s)| s);
        workloads
            .iter()
            .find(|w| {
                selector.is_some_and(|s| {
                    !s.is_empty() && s.iter().all(|(k, v)| w.labels.get(k) == Some(v))
                })
            })
            .map_or_else(|| super::slug(service), |w| w.id.clone())
    };
    let mut addresses: Vec<String> = services.iter().map(|(n, _)| n.clone()).collect();
    addresses.extend(workloads.iter().map(|w| w.id.clone()));
    for w in &workloads {
        for value in &w.env {
            for service in mentioned(value, &addresses) {
                b.connect(&w.id, &target(service));
            }
        }
    }
    for (name, backends) in &ingresses {
        let mut n = node(name, "load-balancer", None);
        n.description = "Kubernetes Ingress".into();
        let id = n.id.clone();
        b.add(n);
        for service in backends {
            b.connect(&id, &target(service));
        }
    }
}

fn workload(doc: &Value, kind: &str, name: &str, b: &mut Builder) -> Workload {
    let template = if kind == "CronJob" {
        at(doc, &["spec", "jobTemplate", "spec", "template"])
    } else {
        at(doc, &["spec", "template"])
    };
    let container = template
        .and_then(|t| at(t, &["spec", "containers"]))
        .and_then(Value::as_sequence)
        .and_then(|c| c.first());
    let image = container
        .and_then(|c| c.get("image"))
        .and_then(Value::as_str);
    let (guessed, tech) = guess(image.unwrap_or(name));
    let component = match kind {
        "CronJob" => "cron",
        "Job" => "worker",
        _ => guessed,
    };
    let pod_labels = labels(template.and_then(|t| t.get("metadata")), "labels");
    let mut n = node(name, component, tech);
    let app = pod_labels.get("app").filter(|a| **a != n.id).cloned();
    n.deploy = Some(Deploy {
        target: DeployTarget::Pod,
        image: image.map(str::to_owned),
        replicas: match kind {
            "Deployment" | "StatefulSet" => Some(
                at(doc, &["spec", "replicas"])
                    .and_then(Value::as_u64)
                    .and_then(|r| u32::try_from(r).ok())
                    .unwrap_or(1),
            ),
            _ => None,
        },
        selector: app,
        port: container
            .and_then(|c| c.get("ports"))
            .and_then(Value::as_sequence)
            .and_then(|p| p.first())
            .and_then(|p| p.get("containerPort"))
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok()),
        health: container.and_then(probe),
    });
    let env = container
        .and_then(|c| c.get("env"))
        .and_then(Value::as_sequence)
        .map(|vars| {
            vars.iter()
                .filter_map(|v| v.get("value").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let id = n.id.clone();
    b.add(n);
    Workload {
        id,
        labels: pod_labels,
        env,
    }
}

/// `readiness: GET /healthz`, `liveness probe`… from the container's probes.
fn probe(container: &Value) -> Option<String> {
    ["readinessProbe", "livenessProbe", "startupProbe"]
        .into_iter()
        .find_map(|key| {
            let probe = container.get(key)?;
            let what = key.trim_end_matches("Probe");
            Some(match str_at(probe, &["httpGet", "path"]) {
                Some(path) => format!("{what}: GET {path}"),
                None => format!("{what} probe"),
            })
        })
}

/// The Services an Ingress routes to.
fn backends(doc: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let rules = at(doc, &["spec", "rules"]).and_then(Value::as_sequence);
    for rule in rules.into_iter().flatten() {
        let paths = at(rule, &["http", "paths"]).and_then(Value::as_sequence);
        for path in paths.into_iter().flatten() {
            let name = str_at(path, &["backend", "service", "name"])
                .or_else(|| str_at(path, &["backend", "serviceName"]));
            if let Some(name) = name
                && !out.iter().any(|n| n == name)
            {
                out.push(name.to_owned());
            }
        }
    }
    out
}

fn at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |v, key| v.get(*key))
}

fn str_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    at(value, path).and_then(Value::as_str)
}

fn labels(parent: Option<&Value>, key: &str) -> Labels {
    parent
        .and_then(|p| p.get(key))
        .and_then(Value::as_mapping)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.as_str()?.to_owned(), v.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}
