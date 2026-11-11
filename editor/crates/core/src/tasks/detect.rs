//! Finding tasks in a project folder.

use std::path::Path;

use serde_json::Value;

use super::{Task, TaskGroup};

/// Every task found in `root`, `.vscode/tasks.json` first (the project's
/// own choice), then Cargo, Go, package.json scripts and Makefile targets.
pub fn detect(root: &Path) -> Vec<Task> {
    let mut tasks = Vec::new();
    vscode(root, &mut tasks);
    cargo(root, &mut tasks);
    go(root, &mut tasks);
    node(root, &mut tasks);
    make(root, &mut tasks);
    // Only one default build task.
    let mut seen_default = false;
    for task in &mut tasks {
        if task.default_build {
            if seen_default {
                task.default_build = false;
            }
            seen_default = true;
        }
    }
    tasks
}

fn task(root: &Path, source: &'static str, group: TaskGroup, command: &str) -> Task {
    Task {
        label: command.to_string(),
        command: command.to_string(),
        cwd: root.to_path_buf(),
        group,
        source,
        default_build: false,
    }
}

fn cargo(root: &Path, out: &mut Vec<Task>) {
    if !root.join("Cargo.toml").is_file() {
        return;
    }
    let mut build = task(root, "cargo", TaskGroup::Build, "cargo build");
    build.default_build = true;
    out.push(build);
    out.push(task(root, "cargo", TaskGroup::Build, "cargo check"));
    out.push(task(root, "cargo", TaskGroup::Test, "cargo test"));
    out.push(task(root, "cargo", TaskGroup::Run, "cargo run"));
    out.push(task(
        root,
        "cargo",
        TaskGroup::Lint,
        "cargo clippy --all-targets",
    ));
    out.push(task(
        root,
        "cargo",
        TaskGroup::Build,
        "cargo build --release",
    ));
}

fn go(root: &Path, out: &mut Vec<Task>) {
    if !root.join("go.mod").is_file() && !root.join("go.work").is_file() {
        return;
    }
    let mut build = task(root, "go", TaskGroup::Build, "go build ./...");
    build.default_build = true;
    out.push(build);
    out.push(task(root, "go", TaskGroup::Test, "go test ./..."));
    out.push(task(root, "go", TaskGroup::Lint, "go vet ./..."));
    out.push(task(root, "go", TaskGroup::Run, "go run ."));
}

/// The package manager a project uses, from its lock file.
fn package_manager(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if root.join("yarn.lock").is_file() {
        "yarn"
    } else if root.join("bun.lockb").is_file() || root.join("bun.lock").is_file() {
        "bun"
    } else {
        "npm"
    }
}

fn node(root: &Path, out: &mut Vec<Task>) {
    let Ok(source) = std::fs::read_to_string(root.join("package.json")) else {
        return;
    };
    let Ok(package) = serde_json::from_str::<Value>(&source) else {
        return;
    };
    let pm = package_manager(root);
    let scripts = package["scripts"].as_object();
    let mut has_build = false;
    for name in scripts.into_iter().flat_map(|s| s.keys()) {
        let group = match name.as_str() {
            "build" | "compile" | "typecheck" | "type-check" | "tsc" => TaskGroup::Build,
            n if n.starts_with("build:") => TaskGroup::Build,
            "test" | "e2e" | "test:e2e" | "test:unit" => TaskGroup::Test,
            n if n.starts_with("test:") => TaskGroup::Test,
            "dev" | "start" | "serve" | "preview" | "start:dev" => TaskGroup::Run,
            "lint" | "format" | "lint:fix" | "check" => TaskGroup::Lint,
            _ => TaskGroup::Other,
        };
        let command = format!("{pm} run {name}");
        let mut t = task(root, "npm", group, &command);
        if name == "build" {
            t.default_build = true;
            has_build = true;
        }
        out.push(t);
    }
    // TypeScript without a build script: a type check still finds errors.
    if !has_build && root.join("tsconfig.json").is_file() {
        let runner = match pm {
            "pnpm" => "pnpm exec",
            "yarn" => "yarn",
            "bun" => "bunx",
            _ => "npx",
        };
        let mut t = task(
            root,
            "npm",
            TaskGroup::Build,
            &format!("{runner} tsc --noEmit"),
        );
        t.default_build = true;
        out.push(t);
    }
}

fn make(root: &Path, out: &mut Vec<Task>) {
    let Some(source) = ["Makefile", "makefile", "GNUmakefile"]
        .iter()
        .find_map(|n| std::fs::read_to_string(root.join(n)).ok())
    else {
        return;
    };
    let mut first = true;
    for line in source.lines() {
        // "target: deps" at the start of a line; skip variables, patterns
        // and special targets.
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        if rest.starts_with('=')
            || name.is_empty()
            || name.starts_with(['.', '\t', ' ', '#'])
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/'))
        {
            continue;
        }
        let group = match name {
            "build" | "all" => TaskGroup::Build,
            "test" | "check" => TaskGroup::Test,
            "run" | "serve" | "dev" => TaskGroup::Run,
            "lint" | "fmt" => TaskGroup::Lint,
            _ => TaskGroup::Other,
        };
        let mut t = task(root, "make", group, &format!("make {name}"));
        // `make` alone runs the first target.
        if first && group == TaskGroup::Build {
            t.default_build = true;
        }
        first = false;
        if !out.iter().any(|o| o.command == t.command) {
            out.push(t);
        }
    }
}

/// Shell and process tasks from `.vscode/tasks.json`.
fn vscode(root: &Path, out: &mut Vec<Task>) {
    let Ok(source) = std::fs::read_to_string(root.join(".vscode").join("tasks.json")) else {
        return;
    };
    let Ok(doc) = crate::theme::jsonc::parse(&source) else {
        return;
    };
    for raw in doc["tasks"].as_array().into_iter().flatten() {
        let kind = raw["type"].as_str().unwrap_or("shell");
        if kind != "shell" && kind != "process" {
            continue;
        }
        let Some(command) = raw["command"].as_str() else {
            continue;
        };
        let args: Vec<String> = raw["args"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a.as_str().or_else(|| a["value"].as_str()))
            .map(shell_quote)
            .collect();
        let command_line = std::iter::once(command.to_string())
            .chain(args)
            .collect::<Vec<_>>()
            .join(" ");
        let (group, default) = match &raw["group"] {
            Value::String(g) => (group_named(g), false),
            Value::Object(g) => (
                group_named(g.get("kind").and_then(Value::as_str).unwrap_or("")),
                g.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
            ),
            _ => (TaskGroup::Other, false),
        };
        let cwd = raw["options"]["cwd"]
            .as_str()
            .map(|c| c.replace("${workspaceFolder}", &root.to_string_lossy()))
            .map_or_else(|| root.to_path_buf(), |c| root.join(c));
        out.push(Task {
            label: raw["label"]
                .as_str()
                .map_or_else(|| command_line.clone(), str::to_string),
            command: command_line.replace("${workspaceFolder}", &root.to_string_lossy()),
            cwd,
            group,
            source: "vscode",
            default_build: default && group == TaskGroup::Build,
        });
    }
}

fn group_named(name: &str) -> TaskGroup {
    match name {
        "build" => TaskGroup::Build,
        "test" => TaskGroup::Test,
        _ => TaskGroup::Other,
    }
}

fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,${}".contains(c))
    {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::default_build;

    fn write(dir: &Path, name: &str, text: &str) {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }

    fn labels(tasks: &[Task]) -> Vec<&str> {
        tasks.iter().map(|t| t.label.as_str()).collect()
    }

    #[test]
    fn cargo_and_go_projects() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "Cargo.toml", "[package]");
        let tasks = detect(dir.path());
        assert_eq!(default_build(&tasks).expect("build").command, "cargo build");
        assert!(labels(&tasks).contains(&"cargo test"));
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "go.mod", "module x");
        let tasks = detect(dir.path());
        assert_eq!(
            default_build(&tasks).expect("build").command,
            "go build ./..."
        );
    }

    #[test]
    fn package_scripts_use_the_projects_package_manager() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            dir.path(),
            "package.json",
            r#"{"scripts": {"dev": "vite", "build": "vite build", "test": "vitest", "lint": "eslint ."}}"#,
        );
        write(dir.path(), "pnpm-lock.yaml", "");
        let tasks = detect(dir.path());
        let build = default_build(&tasks).expect("build");
        assert_eq!(build.command, "pnpm run build");
        let dev = tasks
            .iter()
            .find(|t| t.label == "pnpm run dev")
            .expect("dev");
        assert_eq!(dev.group, TaskGroup::Run);
        // TypeScript without a build script gets a type check.
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "package.json", r#"{"scripts": {"dev": "x"}}"#);
        write(dir.path(), "tsconfig.json", "{}");
        let tasks = detect(dir.path());
        assert_eq!(
            default_build(&tasks).expect("build").command,
            "npx tsc --noEmit"
        );
    }

    #[test]
    fn makefile_targets() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(
            dir.path(),
            "Makefile",
            "CC := gcc\nall: main\n\tcc main.c\ntest:\n\t./t\n.PHONY: all test\n%.o: %.c\n",
        );
        let tasks = detect(dir.path());
        assert_eq!(labels(&tasks), vec!["make all", "make test"]);
        assert!(tasks[0].default_build);
    }

    #[test]
    fn vscode_tasks_win_and_keep_one_default() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "Cargo.toml", "");
        write(
            dir.path(),
            ".vscode/tasks.json",
            r#"{
              // comments are fine
              "version": "2.0.0",
              "tasks": [
                {"label": "Build all", "type": "shell", "command": "cargo",
                 "args": ["build", "--workspace", "it's"],
                 "group": {"kind": "build", "isDefault": true}},
                {"label": "Unit", "type": "process", "command": "cargo", "args": ["test"], "group": "test"},
                {"label": "npm thing", "type": "npm", "script": "x"},
              ]
            }"#,
        );
        let tasks = detect(dir.path());
        assert_eq!(tasks[0].label, "Build all");
        assert_eq!(tasks[0].command, r"cargo build --workspace 'it'\''s'");
        assert_eq!(tasks[1].group, TaskGroup::Test);
        assert_eq!(tasks.iter().filter(|t| t.default_build).count(), 1);
        assert_eq!(default_build(&tasks).expect("build").label, "Build all");
    }
}
