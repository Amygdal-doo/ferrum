//! docker-compose files: a component per service (Docker containers),
//! connected by `depends_on`, `links` and addresses in `environment`.

use std::path::Path;

use serde_yaml_ng::Value;

use super::{Builder, guess, manifests, mentioned, node};
use crate::model::{Deploy, DeployTarget};

/// `docker-compose.yml`, `compose.yaml`, `docker-compose.prod.yml`…
pub(super) fn is_compose(file: &str) -> bool {
    file.starts_with("docker-compose") || file.starts_with("compose.")
}

/// Adds the services of the compose file at `path`; `false` when it has none.
pub(super) fn read(path: &Path, text: &str, b: &mut Builder) -> bool {
    let Ok(doc) = serde_yaml_ng::from_str::<Value>(text) else {
        return false;
    };
    let Some(services) = doc.get("services").and_then(Value::as_mapping) else {
        return false;
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    let names: Vec<String> = services
        .keys()
        .filter_map(|k| k.as_str().map(super::slug))
        .collect();
    for (name, svc) in services {
        let Some(name) = name.as_str() else {
            continue;
        };
        let image = svc.get("image").and_then(Value::as_str);
        let built = build_dir(svc).map(|d| dir.join(d));
        let (kind, tech) = match (image, built.as_deref().and_then(manifests::tech_of)) {
            // Built from the project: what its manifest says.
            (_, Some((kind, tech))) => (kind, Some(tech.to_owned())),
            (Some(image), None) => guess(image),
            (None, None) => guess(name),
        };
        let mut n = node(name, kind, tech);
        n.deploy = Some(Deploy {
            target: DeployTarget::Container,
            image: image.map(str::to_owned),
            replicas: svc
                .get("deploy")
                .and_then(|d| d.get("replicas"))
                .and_then(Value::as_u64)
                .and_then(|r| u32::try_from(r).ok()),
            selector: Some(
                svc.get("container_name")
                    .and_then(Value::as_str)
                    .unwrap_or(name)
                    .to_owned(),
            ),
            port: svc
                .get("ports")
                .and_then(Value::as_sequence)
                .and_then(|p| p.first())
                .and_then(container_port),
            health: health(svc),
        });
        let id = n.id.clone();
        b.add(n);
        for dep in list_or_keys(svc.get("depends_on")) {
            b.connect(&id, &super::slug(&dep));
        }
        for link in list_or_keys(svc.get("links")) {
            let target = link.split(':').next().unwrap_or(&link);
            b.connect(&id, &super::slug(target));
        }
        for value in environment(svc) {
            for target in mentioned(&value, &names) {
                b.connect(&id, target);
            }
        }
    }
    true
}

/// `build: ./api` or `build: { context: ./api }`.
fn build_dir(svc: &Value) -> Option<&str> {
    let build = svc.get("build")?;
    build
        .as_str()
        .or_else(|| build.get("context").and_then(Value::as_str))
}

/// The container's side of `"8080:80"`, `"127.0.0.1:80:80/tcp"`, `80` or
/// `{ target: 80 }`.
fn container_port(port: &Value) -> Option<u16> {
    match port {
        Value::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        Value::String(s) => s
            .rsplit(':')
            .next()?
            .split('/')
            .next()?
            .split('-')
            .next()?
            .parse()
            .ok(),
        Value::Mapping(_) => port
            .get("target")
            .and_then(Value::as_u64)
            .and_then(|n| u16::try_from(n).ok()),
        _ => None,
    }
}

/// The health check's command, unless there is none or it is disabled.
fn health(svc: &Value) -> Option<String> {
    let check = svc.get("healthcheck")?;
    if check.get("disable").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let test = check.get("test")?;
    let text = match test {
        Value::String(s) => s.clone(),
        Value::Sequence(parts) => parts
            .iter()
            .filter_map(Value::as_str)
            .filter(|p| !matches!(*p, "CMD" | "CMD-SHELL"))
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    (!text.is_empty() && text != "NONE").then_some(text)
}

/// The names in `depends_on: [a, b]` or `depends_on: { a: …, b: … }`.
fn list_or_keys(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Sequence(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        Some(Value::Mapping(map)) => map
            .keys()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// The values of `environment: [K=V]` or `environment: { K: V }`.
fn environment(svc: &Value) -> Vec<String> {
    match svc.get("environment") {
        Some(Value::Sequence(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|kv| kv.split_once('=').map(|(_, v)| v.to_owned()))
            .collect(),
        Some(Value::Mapping(map)) => map
            .values()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}
