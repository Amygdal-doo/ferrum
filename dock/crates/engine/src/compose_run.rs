//! A compose project's file and commands for the Compose tab: which file
//! to edit, `docker compose config` to validate it (also unsaved text,
//! through stdin), and actions whose output streams line by line.

use crate::cli::{DockerCli, run_input, run_lines};
use crate::compose::{ComposeAction, ComposeProject, base_args, compose_args};
use crate::endpoint::Endpoint;
use crate::error::EngineResult;
use crate::project::COMPOSE_FILE_NAMES;
use std::path::{Path, PathBuf};

/// The file to open for `project`: its first compose file (from the
/// labels) that exists, else the first compose file of its working folder.
#[must_use]
pub fn compose_file(project: &ComposeProject, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let from_labels = project
        .config_files
        .iter()
        .map(PathBuf::from)
        .find(|p| exists(p));
    from_labels.or_else(|| {
        let dir = PathBuf::from(project.working_dir.as_ref()?);
        COMPOSE_FILE_NAMES
            .iter()
            .map(|name| dir.join(name))
            .find(|p| exists(p))
    })
}

/// `compose … config`. With `unsaved`, the edited text of `file` is read
/// from stdin (`-f -`) in its folder, so paths in it resolve as on disk;
/// the project's other files are left out then.
#[must_use]
pub fn config_args(
    project: &ComposeProject,
    file: Option<&Path>,
    unsaved: bool,
    exists: impl Fn(&Path) -> bool,
) -> Vec<String> {
    let mut args = match (unsaved, file) {
        (true, Some(file)) => {
            let dir = file
                .parent()
                .map_or_else(|| ".".to_owned(), |d| d.display().to_string());
            vec![
                "compose".to_owned(),
                "-p".to_owned(),
                project.name.clone(),
                "--project-directory".to_owned(),
                dir,
                "-f".to_owned(),
                "-".to_owned(),
            ]
        }
        (false, Some(file)) if project.config_files.is_empty() => vec![
            "compose".to_owned(),
            "-p".to_owned(),
            project.name.clone(),
            "-f".to_owned(),
            file.display().to_string(),
        ],
        _ => base_args(project, exists),
    };
    args.push("config".to_owned());
    args
}

/// Validates the project's compose configuration; returns the resolved
/// configuration, or compose's error.
pub async fn validate(
    cli: &DockerCli,
    endpoint: &Endpoint,
    project: &ComposeProject,
    file: Option<&Path>,
    unsaved: Option<&str>,
) -> EngineResult<String> {
    let args = config_args(project, file, unsaved.is_some(), Path::exists);
    tracing::info!(project = %project.name, "docker compose config");
    run_input(cli, endpoint, &args, unsaved.unwrap_or("")).await
}

/// Runs `action` for the project, streaming its output to `on_line`.
pub async fn run_streaming(
    cli: &DockerCli,
    endpoint: &Endpoint,
    project: &ComposeProject,
    action: ComposeAction,
    on_line: impl FnMut(String) + Send,
) -> EngineResult<()> {
    let args = compose_args(project, action, Path::exists);
    tracing::info!(project = %project.name, action = action.verb(), "docker compose");
    run_lines(cli, endpoint, &args, on_line).await
}

/// The command line shown above a run's output.
#[must_use]
pub fn command_line(project: &ComposeProject, action: ComposeAction) -> String {
    format!(
        "docker {}",
        compose_args(project, action, Path::exists).join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[&str]) -> ComposeProject {
        ComposeProject {
            name: "shop".into(),
            working_dir: Some("/src/shop".into()),
            config_files: files.iter().map(|f| (*f).to_owned()).collect(),
        }
    }

    #[test]
    fn finds_the_file_to_edit() {
        let labelled = project(&["/src/shop/gone.yaml", "/src/shop/compose.yaml"]);
        let exists = |p: &Path| p != Path::new("/src/shop/gone.yaml");
        assert_eq!(
            compose_file(&labelled, exists),
            Some(PathBuf::from("/src/shop/compose.yaml"))
        );
        let unlabelled = project(&[]);
        assert_eq!(
            compose_file(&unlabelled, |p| p
                == Path::new("/src/shop/docker-compose.yml")),
            Some(PathBuf::from("/src/shop/docker-compose.yml"))
        );
        assert_eq!(compose_file(&unlabelled, |_| false), None);
    }

    #[test]
    fn builds_config_arguments() {
        let p = project(&["/src/shop/compose.yaml"]);
        let file = Path::new("/src/shop/compose.yaml");
        assert_eq!(
            config_args(&p, Some(file), false, |_| true).join(" "),
            "compose -p shop --project-directory /src/shop -f /src/shop/compose.yaml config"
        );
        assert_eq!(
            config_args(&p, Some(file), true, |_| true).join(" "),
            "compose -p shop --project-directory /src/shop -f - config"
        );
        let unlabelled = project(&[]);
        assert_eq!(
            config_args(&unlabelled, Some(file), false, |_| true).join(" "),
            "compose -p shop -f /src/shop/compose.yaml config"
        );
    }

    /// A stand-in `docker` that prints to both streams and exits with `code`.
    #[cfg(unix)]
    fn fake_docker(dir: &Path, script: &str) -> DockerCli {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("docker");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap_or_else(|e| panic!("{e}"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|e| panic!("{e}"));
        DockerCli { path }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn streams_output_and_reports_failures() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let endpoint = Endpoint::Unix("/tmp/x.sock".into());
        let ok = fake_docker(
            dir.path(),
            r#"echo "args: $*"; printf ' Pulling 10%%\r Pulling 100%%\n' >&2; echo "host=$DOCKER_HOST""#,
        );
        let mut lines = Vec::new();
        let result = run_streaming(&ok, &endpoint, &project(&[]), ComposeAction::Pull, |l| {
            lines.push(l);
        })
        .await;
        assert!(result.is_ok(), "{result:?}");
        assert!(
            lines.contains(&"args: compose -p shop pull".to_owned()),
            "{lines:?}"
        );
        assert!(lines.contains(&" Pulling 100%".to_owned()), "{lines:?}");
        assert!(
            lines.contains(&"host=unix:///tmp/x.sock".to_owned()),
            "{lines:?}"
        );

        let failing = fake_docker(
            dir.path(),
            "echo 'service \"web\" refers to undefined volume data' >&2; exit 15",
        );
        let result = run_streaming(
            &failing,
            &endpoint,
            &project(&[]),
            ComposeAction::Up,
            |_| {},
        )
        .await;
        assert!(result.is_err_and(|e| e.to_string().contains("undefined volume")));

        let echo_stdin = fake_docker(dir.path(), "echo \"$*\"; cat");
        let checked = validate(
            &echo_stdin,
            &endpoint,
            &project(&[]),
            Some(Path::new("/src/shop/compose.yaml")),
            Some("services: {}\n"),
        )
        .await;
        assert_eq!(
            checked.ok().as_deref(),
            Some("compose -p shop --project-directory /src/shop -f - config\nservices: {}")
        );
    }
}
