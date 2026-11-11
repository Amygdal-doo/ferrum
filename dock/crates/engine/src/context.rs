//! Docker CLI contexts (`docker context ls`): read from the CLI's config
//! folder (`$DOCKER_CONFIG` or `~/.docker`). The current one is
//! `$DOCKER_CONTEXT`, else `currentContext` in `config.json`; each context's
//! engine address is in `contexts/meta/<hash>/meta.json`.

use crate::endpoint::Endpoint;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerContext {
    pub name: String,
    /// The raw `Host`, e.g. `unix:///…/docker.sock` or `ssh://me@box`.
    pub host: String,
    /// `None` when FerrumDock cannot talk to it (ssh, npipe, TLS-only…).
    pub endpoint: Option<Endpoint>,
    pub current: bool,
}

/// `currentContext` from `config.json` (`None` or `"default"` = no context).
#[must_use]
pub fn current_context(config_json: &str) -> Option<String> {
    let config: serde_json::Value = serde_json::from_str(config_json).ok()?;
    config
        .get("currentContext")
        .and_then(serde_json::Value::as_str)
        .filter(|c| !c.is_empty() && *c != "default")
        .map(str::to_owned)
}

/// (name, docker host) from a context's `meta.json`.
#[must_use]
pub fn parse_meta(meta_json: &str) -> Option<(String, String)> {
    let meta: serde_json::Value = serde_json::from_str(meta_json).ok()?;
    let name = meta.get("Name")?.as_str()?.to_owned();
    let host = meta.pointer("/Endpoints/docker/Host")?.as_str()?.to_owned();
    Some((name, host))
}

/// The CLI's config folder: `$DOCKER_CONFIG`, else `~/.docker`.
#[must_use]
pub fn config_dir(docker_config: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    docker_config
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".docker")))
}

/// Every context under `dir`, sorted by name, the current one marked.
/// `env_context` is `$DOCKER_CONTEXT`, which wins over `config.json`.
#[must_use]
pub fn load(dir: &Path, env_context: Option<&str>) -> Vec<DockerContext> {
    let current = env_context
        .filter(|c| !c.is_empty() && *c != "default")
        .map(str::to_owned)
        .or_else(|| {
            std::fs::read_to_string(dir.join("config.json"))
                .ok()
                .and_then(|text| current_context(&text))
        });
    let Ok(entries) = std::fs::read_dir(dir.join("contexts").join("meta")) else {
        return Vec::new();
    };
    let mut contexts: Vec<DockerContext> = entries
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path().join("meta.json")).ok())
        .filter_map(|text| parse_meta(&text))
        .map(|(name, host)| DockerContext {
            current: current.as_deref() == Some(name.as_str()),
            endpoint: Endpoint::parse(&host),
            name,
            host,
        })
        .collect();
    contexts.sort_by(|a, b| a.name.cmp(&b.name));
    contexts
}

/// Contexts on this machine, from the real environment.
#[must_use]
pub fn detect() -> Vec<DockerContext> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let docker_config = std::env::var("DOCKER_CONFIG").ok();
    let env_context = std::env::var("DOCKER_CONTEXT").ok();
    config_dir(docker_config.as_deref(), home.as_deref())
        .map(|dir| load(&dir, env_context.as_deref()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_current_context_and_meta() {
        assert_eq!(
            current_context(r#"{"auths":{},"currentContext":"orbstack"}"#),
            Some("orbstack".into())
        );
        assert_eq!(current_context(r#"{"currentContext":"default"}"#), None);
        assert_eq!(current_context("not json"), None);
        let meta = r#"{"Name":"colima","Metadata":{},"Endpoints":{"docker":{"Host":"unix:///Users/me/.colima/default/docker.sock","SkipTLSVerify":false}}}"#;
        assert_eq!(
            parse_meta(meta),
            Some((
                "colima".into(),
                "unix:///Users/me/.colima/default/docker.sock".into()
            ))
        );
        assert_eq!(parse_meta(r#"{"Name":"x","Endpoints":{}}"#), None);
        assert_eq!(
            config_dir(None, Some(Path::new("/h"))),
            Some(PathBuf::from("/h/.docker"))
        );
        assert_eq!(config_dir(Some("/c"), None), Some(PathBuf::from("/c")));
    }

    #[test]
    fn loads_contexts_from_the_config_folder() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let write = |rel: &str, text: &str| {
            let path = dir.path().join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir: {e}"));
            }
            std::fs::write(path, text).unwrap_or_else(|e| panic!("write: {e}"));
        };
        write("config.json", r#"{"currentContext":"orbstack"}"#);
        write(
            "contexts/meta/aaa/meta.json",
            r#"{"Name":"orbstack","Endpoints":{"docker":{"Host":"unix:///Users/me/.orbstack/run/docker.sock"}}}"#,
        );
        write(
            "contexts/meta/bbb/meta.json",
            r#"{"Name":"remote","Endpoints":{"docker":{"Host":"ssh://me@box"}}}"#,
        );
        write("contexts/meta/ccc/meta.json", "broken");
        let contexts = load(dir.path(), None);
        let names: Vec<_> = contexts.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["orbstack", "remote"]);
        assert!(contexts[0].current && contexts[0].endpoint.is_some());
        assert!(!contexts[1].current && contexts[1].endpoint.is_none());
        let env = load(dir.path(), Some("remote"));
        assert!(env[1].current && !env[0].current);
        assert!(load(&dir.path().join("missing"), None).is_empty());
    }
}
