//! Launch configurations: `.vscode/launch.json` (JSONC, `${…}` variables
//! substituted) for the types we can debug, and sensible defaults when
//! there is none: the Rust binary or tests (built with cargo first), the
//! current Go package or its tests, the current JS/TS file or a
//! package.json script.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// `type` values we run.
pub const SUPPORTED_TYPES: &[&str] = &[
    "lldb",
    "lldb-dap",
    "codelldb",
    "go",
    "node",
    "pwa-node",
    "node-terminal",
];

#[derive(Debug, Clone, PartialEq)]
pub struct LaunchConfig {
    pub name: String,
    /// The `type`.
    pub kind: String,
    /// `launch` or `attach`.
    pub request: String,
    /// The whole configuration, variables substituted.
    pub raw: Value,
    /// `launch.json` or `default`.
    pub source: &'static str,
}

/// What `${file}` and friends stand for.
#[derive(Debug, Clone, Default)]
pub struct Variables {
    pub root: PathBuf,
    pub file: Option<PathBuf>,
}

impl Variables {
    fn get(&self, name: &str) -> Option<String> {
        let s = |p: &Path| p.to_string_lossy().to_string();
        let file = self.file.as_deref();
        Some(match name {
            "workspaceFolder" | "workspaceRoot" | "cwd" => s(&self.root),
            "workspaceFolderBasename" => self
                .root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())?,
            "file" => s(file?),
            "fileDirname" => s(file?.parent()?),
            "fileBasename" => file?.file_name()?.to_string_lossy().to_string(),
            "fileBasenameNoExtension" => file?.file_stem()?.to_string_lossy().to_string(),
            "fileExtname" => format!(".{}", file?.extension()?.to_string_lossy()),
            "relativeFile" => s(file?.strip_prefix(&self.root).ok()?),
            "relativeFileDirname" => s(file?.parent()?.strip_prefix(&self.root).ok()?),
            "pathSeparator" => std::path::MAIN_SEPARATOR.to_string(),
            name => std::env::var(name.strip_prefix("env:")?).unwrap_or_default(),
        })
    }

    /// `text` with every known `${name}` replaced (unknown ones stay).
    pub fn substitute(&self, text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find("${") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            match after.find('}') {
                Some(end) => {
                    let name = &after[..end];
                    match self.get(name) {
                        Some(value) => out.push_str(&value),
                        None => out.push_str(&rest[start..start + 3 + end]),
                    }
                    rest = &after[end + 1..];
                }
                None => {
                    out.push_str(&rest[start..]);
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    }

    pub fn substitute_value(&self, value: &mut Value) {
        match value {
            Value::String(s) => *s = self.substitute(s),
            Value::Array(items) => items.iter_mut().for_each(|v| self.substitute_value(v)),
            Value::Object(map) => map.values_mut().for_each(|v| self.substitute_value(v)),
            _ => {}
        }
    }
}

/// Supported configurations in `<root>/.vscode/launch.json`.
pub fn read_launch_json(vars: &Variables) -> Result<Vec<LaunchConfig>, String> {
    let path = vars.root.join(".vscode").join("launch.json");
    let Ok(source) = std::fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    let doc = crate::theme::jsonc::parse(&source).map_err(|e| format!("launch.json: {e}"))?;
    Ok(doc["configurations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|raw| {
            let kind = raw["type"].as_str()?;
            if !SUPPORTED_TYPES.contains(&kind) {
                return None;
            }
            let mut raw = raw.clone();
            vars.substitute_value(&mut raw);
            Some(LaunchConfig {
                name: raw["name"].as_str().unwrap_or(kind).to_string(),
                kind: kind.to_string(),
                request: raw["request"].as_str().unwrap_or("launch").to_string(),
                raw,
                source: "launch.json",
            })
        })
        .collect())
}

fn config(name: &str, kind: &str, raw: Value) -> LaunchConfig {
    let mut raw = raw;
    raw["name"] = json!(name);
    raw["type"] = json!(kind);
    raw["request"] = json!("launch");
    LaunchConfig {
        name: name.to_string(),
        kind: kind.to_string(),
        request: "launch".into(),
        raw,
        source: "default",
    }
}

/// Configurations for a project without launch.json. The current file's
/// language comes first.
pub fn default_configs(vars: &Variables) -> Vec<LaunchConfig> {
    let root = &vars.root;
    let file = vars.file.as_deref();
    let ext = file
        .and_then(Path::extension)
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let cwd = root.to_string_lossy().to_string();
    let mut rust = Vec::new();
    if root.join("Cargo.toml").is_file() {
        rust.push(config(
            "Rust: debug binary (cargo build)",
            "lldb",
            json!({"cargo": {"args": ["build"]}, "args": [], "cwd": cwd}),
        ));
        rust.push(config(
            "Rust: debug tests (cargo test --no-run)",
            "lldb",
            json!({"cargo": {"args": ["test", "--no-run"]}, "args": [], "cwd": cwd}),
        ));
    }
    let mut go = Vec::new();
    if root.join("go.mod").is_file() || root.join("go.work").is_file() || ext == "go" {
        let package = match file.filter(|_| ext == "go").and_then(Path::parent) {
            Some(dir) => dir.to_string_lossy().to_string(),
            None => cwd.clone(),
        };
        go.push(config(
            "Go: debug package",
            "go",
            json!({"mode": "debug", "program": package, "cwd": cwd}),
        ));
        go.push(config(
            "Go: debug tests",
            "go",
            json!({"mode": "test", "program": package, "cwd": cwd}),
        ));
    }
    let mut js = Vec::new();
    if let Some(file) = file.filter(|_| matches!(ext.as_str(), "js" | "mjs" | "cjs" | "ts" | "mts"))
    {
        let name = format!(
            "Node: {}",
            file.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default()
        );
        js.push(config(
            &name,
            "pwa-node",
            json!({"program": file.to_string_lossy(), "cwd": cwd}),
        ));
    }
    if let Ok(source) = std::fs::read_to_string(root.join("package.json"))
        && let Ok(package) = serde_json::from_str::<Value>(&source)
    {
        let pm = if root.join("pnpm-lock.yaml").is_file() {
            "pnpm"
        } else if root.join("yarn.lock").is_file() {
            "yarn"
        } else {
            "npm"
        };
        for script in package["scripts"]
            .as_object()
            .into_iter()
            .flat_map(Map::keys)
        {
            js.push(config(
                &format!("{pm} run {script}"),
                "pwa-node",
                json!({"runtimeExecutable": pm, "runtimeArgs": ["run", script], "cwd": cwd}),
            ));
        }
    }
    let mut out = Vec::new();
    match ext.as_str() {
        "go" => out.append(&mut go),
        "js" | "mjs" | "cjs" | "ts" | "mts" | "tsx" | "jsx" => out.append(&mut js),
        _ => out.append(&mut rust),
    }
    out.extend(rust);
    out.extend(go);
    out.extend(js);
    out
}

// ----- Cargo ------------------------------------------------------------------

/// `cargo <args>` with JSON messages, for finding the built program.
pub fn cargo_args(config: &Value) -> Option<Vec<String>> {
    let args: Vec<String> = config["cargo"]["args"]
        .as_array()?
        .iter()
        .filter_map(|a| a.as_str().map(str::to_string))
        .collect();
    let mut args = if args.is_empty() {
        vec!["build".to_string()]
    } else {
        args
    };
    if !args.iter().any(|a| a.starts_with("--message-format")) {
        args.push("--message-format=json".into());
    }
    Some(args)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub name: String,
    pub kinds: Vec<String>,
    pub test: bool,
    pub executable: PathBuf,
}

/// Executables from cargo's `--message-format=json` output.
pub fn cargo_executables(output: &str) -> Vec<Artifact> {
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|m| m["reason"] == "compiler-artifact")
        .filter_map(|m| {
            Some(Artifact {
                name: m["target"]["name"].as_str()?.to_string(),
                kinds: m["target"]["kind"]
                    .as_array()?
                    .iter()
                    .filter_map(|k| k.as_str().map(str::to_string))
                    .collect(),
                test: m["profile"]["test"].as_bool().unwrap_or(false),
                executable: PathBuf::from(m["executable"].as_str()?),
            })
        })
        .collect()
}

/// The program to debug: with `cargo.filter` (`name` / `kind`) the
/// matching artifact, else the only (or first) one — a binary for builds,
/// a test executable for `test --no-run`.
pub fn pick_executable(config: &Value, artifacts: &[Artifact]) -> Option<PathBuf> {
    let filter = &config["cargo"]["filter"];
    let tests = config["cargo"]["args"]
        .as_array()
        .is_some_and(|a| a.iter().any(|x| x == "test"));
    artifacts
        .iter()
        .filter(|a| filter["name"].as_str().is_none_or(|n| n == a.name))
        .filter(|a| {
            filter["kind"]
                .as_str()
                .is_none_or(|k| a.kinds.iter().any(|x| x == k))
        })
        .find(|a| a.test == tests && (tests || a.kinds.iter().any(|k| k == "bin")))
        .or_else(|| artifacts.first())
        .map(|a| a.executable.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(root: &Path, file: Option<&str>) -> Variables {
        Variables {
            root: root.to_path_buf(),
            file: file.map(|f| root.join(f)),
        }
    }

    #[test]
    fn substitutes_variables() {
        let v = vars(Path::new("/p/app"), Some("src/main.rs"));
        assert_eq!(v.substitute("${workspaceFolder}/target"), "/p/app/target");
        assert_eq!(
            v.substitute("${fileBasenameNoExtension}:${relativeFile}"),
            "main:src/main.rs"
        );
        assert_eq!(v.substitute("${unknown} ${"), "${unknown} ${");
        let mut value = json!({"args": ["${fileDirname}"], "n": 1});
        v.substitute_value(&mut value);
        assert_eq!(value["args"][0], "/p/app/src");
    }

    #[test]
    fn reads_supported_launch_json_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join(".vscode")).expect("mkdir");
        std::fs::write(
            dir.path().join(".vscode/launch.json"),
            r#"{
              // comments are fine
              "configurations": [
                {"name": "Run", "type": "lldb", "request": "launch", "program": "${workspaceFolder}/target/debug/app",},
                {"name": "Py", "type": "python", "request": "launch"},
                {"name": "Attach", "type": "go", "request": "attach", "processId": 42},
              ],
            }"#,
        )
        .expect("write");
        let configs = read_launch_json(&vars(dir.path(), None)).expect("parse");
        assert_eq!(configs.len(), 2);
        assert_eq!(
            configs[0].raw["program"],
            format!("{}/target/debug/app", dir.path().display())
        );
        assert_eq!(configs[1].request, "attach");
        assert!(
            read_launch_json(&vars(Path::new("/nonexistent"), None))
                .expect("ok")
                .is_empty()
        );
    }

    #[test]
    fn defaults_follow_the_project_and_current_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join("Cargo.toml"), "").expect("write");
        std::fs::write(root.join("go.mod"), "module x").expect("write");
        std::fs::write(root.join("package.json"), r#"{"scripts": {"dev": "vite"}}"#)
            .expect("write");
        let names = |file| -> Vec<String> {
            default_configs(&vars(root, file))
                .into_iter()
                .map(|c| c.name)
                .collect()
        };
        assert_eq!(names(None)[0], "Rust: debug binary (cargo build)");
        let go = names(Some("cmd/x/main.go"));
        assert_eq!(go[0], "Go: debug package");
        let js = default_configs(&vars(root, Some("src/app.ts")));
        assert_eq!(js[0].name, "Node: app.ts");
        assert_eq!(js[0].kind, "pwa-node");
        assert!(
            js.iter()
                .any(|c| c.name == "npm run dev" && c.raw["runtimeArgs"][1] == "dev")
        );
        let go_pkg = default_configs(&vars(root, Some("cmd/x/main.go")));
        assert_eq!(
            go_pkg[0].raw["program"],
            root.join("cmd/x").to_string_lossy().as_ref()
        );
    }

    #[test]
    fn cargo_artifacts_pick_the_program() {
        let output = r#"{"reason":"compiler-artifact","target":{"name":"dep","kind":["lib"]},"profile":{"test":false},"executable":null}
{"reason":"compiler-artifact","target":{"name":"app","kind":["bin"]},"profile":{"test":false},"executable":"/t/debug/app"}
{"reason":"compiler-artifact","target":{"name":"app","kind":["bin"]},"profile":{"test":true},"executable":"/t/debug/deps/app-1234"}
{"reason":"build-finished","success":true}"#;
        let artifacts = cargo_executables(output);
        assert_eq!(artifacts.len(), 2);
        let build = json!({"cargo": {"args": ["build"]}});
        assert_eq!(
            pick_executable(&build, &artifacts),
            Some(PathBuf::from("/t/debug/app"))
        );
        let test = json!({"cargo": {"args": ["test", "--no-run"]}});
        assert_eq!(
            pick_executable(&test, &artifacts),
            Some(PathBuf::from("/t/debug/deps/app-1234"))
        );
        assert_eq!(
            cargo_args(&build).expect("args"),
            ["build", "--message-format=json"]
        );
        assert!(cargo_args(&json!({})).is_none());
    }
}
