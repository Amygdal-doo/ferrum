//! `docker compose ps --all --format json` for one project.

use crate::cli::{DockerCli, run_capture};
use crate::compose::{ComposeProject, base_args};
use crate::endpoint::Endpoint;
use crate::error::{EngineError, EngineResult};
use crate::model::ContainerState;
use serde::Deserialize;
use std::path::Path;

/// One service container as compose reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceStatus {
    pub service: String,
    pub name: String,
    pub image: String,
    pub state: ContainerState,
    /// e.g. "Up 2 minutes (healthy)", "Exited (0) 3 hours ago".
    pub status: String,
    /// `8080→80/tcp` for each published port.
    pub ports: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Publisher {
    #[serde(default)]
    target_port: u16,
    #[serde(default)]
    published_port: u16,
    #[serde(default)]
    protocol: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Row {
    #[serde(default)]
    name: String,
    #[serde(default)]
    service: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    publishers: Option<Vec<Publisher>>,
}

impl From<Row> for ServiceStatus {
    fn from(row: Row) -> Self {
        let mut ports: Vec<String> = row
            .publishers
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.published_port > 0)
            .map(|p| {
                let proto = if p.protocol.is_empty() {
                    "tcp"
                } else {
                    &p.protocol
                };
                format!("{}→{}/{proto}", p.published_port, p.target_port)
            })
            .collect();
        ports.dedup();
        Self {
            service: row.service,
            name: row.name,
            image: row.image,
            state: ContainerState::parse(&row.state),
            status: row.status,
            ports,
        }
    }
}

/// Parses `compose ps --format json`: a JSON array (compose < 2.21) or one
/// object per line (newer). Other lines (warnings on stderr) are skipped.
pub fn parse_ps(output: &str) -> Result<Vec<ServiceStatus>, String> {
    let mut rows: Vec<Row> = Vec::new();
    let mut parsed_any = false;
    for line in output.lines().map(str::trim) {
        if line.starts_with('[') {
            let list: Vec<Row> = serde_json::from_str(line).map_err(|e| e.to_string())?;
            rows.extend(list);
            parsed_any = true;
        } else if line.starts_with('{') {
            rows.push(serde_json::from_str(line).map_err(|e| e.to_string())?);
            parsed_any = true;
        }
    }
    if !parsed_any && !output.trim().is_empty() {
        return Err(output.trim().to_owned());
    }
    let mut services: Vec<ServiceStatus> = rows.into_iter().map(ServiceStatus::from).collect();
    services.sort_by(|a, b| a.service.cmp(&b.service).then_with(|| a.name.cmp(&b.name)));
    Ok(services)
}

/// Arguments after `docker` for `ps` of `project`.
#[must_use]
pub fn ps_args(project: &ComposeProject, exists: impl Fn(&Path) -> bool) -> Vec<String> {
    let mut args = base_args(project, exists);
    args.extend(["ps", "--all", "--format", "json"].map(str::to_owned));
    args
}

/// Runs `docker compose … ps` against `endpoint`.
pub async fn run_ps(
    cli: &DockerCli,
    endpoint: &Endpoint,
    project: &ComposeProject,
) -> EngineResult<Vec<ServiceStatus>> {
    let output = run_capture(cli, endpoint, &ps_args(project, Path::exists)).await?;
    parse_ps(&output).map_err(EngineError::Command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_line_and_array_formats() {
        let lines = r#"time="x" level=warning msg="version is obsolete"
{"Name":"web-db-1","Service":"db","Image":"postgres:16","State":"running","Status":"Up 2 hours","Publishers":[{"URL":"0.0.0.0","TargetPort":5432,"PublishedPort":5432,"Protocol":"tcp"},{"URL":"::","TargetPort":5432,"PublishedPort":5432,"Protocol":"tcp"}]}
{"Name":"web-app-1","Service":"app","Image":"web-app","State":"exited","Status":"Exited (1) 3 minutes ago","Publishers":null}"#;
        let services = parse_ps(lines).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].service, "app");
        assert_eq!(services[0].state, ContainerState::Exited);
        assert_eq!(services[1].ports, ["5432→5432/tcp"]);

        let array = r#"[{"Name":"a-x-1","Service":"x","State":"running","Publishers":[{"TargetPort":80,"PublishedPort":0}]}]"#;
        let services = parse_ps(array).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(services[0].name, "a-x-1");
        assert!(services[0].ports.is_empty(), "exposed only");

        assert_eq!(parse_ps(""), Ok(Vec::new()));
        assert_eq!(
            parse_ps("no configuration file provided: not found"),
            Err("no configuration file provided: not found".to_owned())
        );
    }

    #[test]
    fn builds_ps_arguments() {
        let project = ComposeProject {
            name: "shop".into(),
            working_dir: Some("/src/shop".into()),
            config_files: vec!["/src/shop/compose.yaml".into()],
        };
        assert_eq!(
            ps_args(&project, |_| true).join(" "),
            "compose -p shop --project-directory /src/shop -f /src/shop/compose.yaml ps --all --format json"
        );
        assert_eq!(
            ps_args(&project, |_| false).join(" "),
            "compose -p shop ps --all --format json"
        );
    }
}
