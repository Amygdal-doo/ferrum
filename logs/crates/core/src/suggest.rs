//! Sources a project probably has: dev scripts, `cargo run`, compose
//! services and log folders.

use crate::glob;
use crate::source::{SourceConfig, SourceKind};
use std::path::Path;

/// `package.json` scripts worth tailing, in the order offered.
const SCRIPTS: [&str; 5] = ["dev", "start", "serve", "watch", "start:dev"];
/// Folders that usually hold log files.
const LOG_DIRS: [&str; 5] = ["logs", "log", "storage/logs", "tmp", "var/log"];
const COMPOSE_FILES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yml",
    "docker-compose.yaml",
];

/// Suggestions for `project`, most likely first, without the ones in `existing`.
#[must_use]
pub fn suggest(project: &Path, existing: &[SourceConfig]) -> Vec<SourceConfig> {
    let mut out = Vec::new();
    node_scripts(project, &mut out);
    if let Ok(text) = std::fs::read_to_string(project.join("Cargo.toml"))
        && text.contains("[package]")
    {
        out.push(command("cargo run", "cargo run"));
    }
    if project.join("manage.py").is_file() {
        out.push(command("runserver", "python manage.py runserver"));
    }
    compose(project, &mut out);
    for dir in LOG_DIRS {
        let pattern = format!("{dir}/*.log");
        if !glob::resolve(project, &pattern, None).is_empty() {
            out.push(SourceConfig::new(
                pattern.clone(),
                SourceKind::File { path: pattern },
            ));
        }
    }
    out.retain(|s| !existing.iter().any(|e| e.same_target(s)));
    out
}

fn command(name: &str, line: &str) -> SourceConfig {
    SourceConfig::new(
        name,
        SourceKind::Command {
            command: line.to_owned(),
        },
    )
}

fn node_scripts(project: &Path, out: &mut Vec<SourceConfig>) {
    let Ok(text) = std::fs::read_to_string(project.join("package.json")) else {
        return;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(scripts) = json.get("scripts").and_then(|s| s.as_object()) else {
        return;
    };
    let runner = if project.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if project.join("yarn.lock").is_file() {
        "yarn"
    } else if project.join("bun.lockb").is_file() || project.join("bun.lock").is_file() {
        "bun run"
    } else {
        "npm run"
    };
    for script in SCRIPTS {
        if scripts.contains_key(script) {
            let line = if runner == "npm run" && script == "start" {
                "npm start".to_owned()
            } else {
                format!("{runner} {script}")
            };
            out.push(command(script, &line));
        }
    }
}

fn compose(project: &Path, out: &mut Vec<SourceConfig>) {
    let Some(text) = COMPOSE_FILES
        .iter()
        .find_map(|name| std::fs::read_to_string(project.join(name)).ok())
    else {
        return;
    };
    out.push(SourceConfig::new(
        "compose",
        SourceKind::Compose {
            service: String::new(),
        },
    ));
    let Ok(doc) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&text) else {
        return;
    };
    let Some(services) = doc.get("services").and_then(|s| s.as_mapping()) else {
        return;
    };
    for name in services.keys().filter_map(|k| k.as_str()) {
        out.push(SourceConfig::new(
            name,
            SourceKind::Compose {
                service: name.to_owned(),
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(list: &[SourceConfig]) -> Vec<String> {
        list.iter()
            .map(|s| format!("{}:{}", s.kind.label(), s.kind.target()))
            .collect()
    }

    #[test]
    fn detects_scripts_cargo_compose_and_log_folders() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(
            root.join("package.json"),
            r#"{"scripts":{"build":"x","start":"node .","dev":"vite"}}"#,
        )
        .expect("package.json");
        std::fs::write(root.join("pnpm-lock.yaml"), "").expect("lock");
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").expect("cargo");
        std::fs::write(
            root.join("compose.yaml"),
            "services:\n  web:\n    image: x\n  db:\n    image: y\n",
        )
        .expect("compose");
        std::fs::create_dir_all(root.join("storage/logs")).expect("mkdir");
        std::fs::write(root.join("storage/logs/laravel.log"), "").expect("log");
        std::fs::create_dir_all(root.join("logs")).expect("mkdir");
        let list = suggest(root, &[]);
        assert_eq!(
            targets(&list),
            [
                "Command:pnpm dev",
                "Command:pnpm start",
                "Command:cargo run",
                "Compose:",
                "Compose:web",
                "Compose:db",
                "File:storage/logs/*.log",
            ]
        );
        // Already added sources are not suggested again.
        let fewer = suggest(root, &list[..2]);
        assert_eq!(fewer.len(), list.len() - 2);
    }

    #[test]
    fn npm_start_and_empty_projects() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(suggest(dir.path(), &[]).is_empty());
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts":{"start":"node ."}}"#,
        )
        .expect("package.json");
        assert_eq!(targets(&suggest(dir.path(), &[])), ["Command:npm start"]);
    }
}
