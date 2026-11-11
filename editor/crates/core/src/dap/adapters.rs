//! Debug adapters per launch `type`, where to find them, how to install
//! the missing ones, and the `launch` / `attach` arguments each expects.
//!
//! | type | adapter | found |
//! |------|---------|-------|
//! | `lldb`, `lldb-dap` | `lldb-dap` (stdio) | `xcrun -f lldb-dap`, PATH (`lldb-dap`, `lldb-vscode`), Homebrew LLVM; CodeLLDB if that is all there is |
//! | `codelldb` | CodeLLDB (TCP) | PATH, the VS Code extension |
//! | `go` | `dlv dap` (TCP) | PATH, `servers/dlv` (installed with `go install`) |
//! | `node`, `pwa-node`, `node-terminal` | js-debug's `dapDebugServer.js` under Node (TCP) | `servers/js-debug` (installed from its GitHub release) |

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::client::{AdapterCommand, Transport};
use crate::lsp::servers::find_program;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterKind {
    LldbDap,
    CodeLldb,
    Delve,
    JsDebug,
}

impl AdapterKind {
    pub fn for_type(kind: &str) -> Option<Self> {
        Some(match kind {
            "lldb" | "lldb-dap" => AdapterKind::LldbDap,
            "codelldb" => AdapterKind::CodeLldb,
            "go" => AdapterKind::Delve,
            "node" | "pwa-node" | "node-terminal" => AdapterKind::JsDebug,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            AdapterKind::LldbDap => "lldb-dap",
            AdapterKind::CodeLldb => "CodeLLDB",
            AdapterKind::Delve => "dlv dap",
            AdapterKind::JsDebug => "js-debug",
        }
    }

    /// `adapterID` sent in `initialize`.
    pub fn adapter_id(self) -> &'static str {
        match self {
            AdapterKind::LldbDap => "lldb-dap",
            AdapterKind::CodeLldb => "lldb",
            AdapterKind::Delve => "go",
            AdapterKind::JsDebug => "pwa-node",
        }
    }
}

/// An adapter that is not installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    pub adapter: AdapterKind,
    /// What the user can do about it.
    pub hint: String,
    /// The editor can install it (Debug: Install Adapter).
    pub installable: bool,
}

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not installed: {}", self.adapter.name(), self.hint)
    }
}

/// `servers/js-debug/js-debug/src/dapDebugServer.js`.
pub fn js_debug_server(servers: &Path) -> PathBuf {
    servers
        .join("js-debug")
        .join("js-debug")
        .join("src")
        .join("dapDebugServer.js")
}

fn xcrun_lldb_dap() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let output = std::process::Command::new("xcrun")
        .args(["-f", "lldb-dap"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    (output.status.success() && path.is_file()).then_some(path)
}

pub fn find_lldb_dap(path_env: &str) -> Option<PathBuf> {
    xcrun_lldb_dap()
        .or_else(|| find_program("lldb-dap", path_env))
        .or_else(|| find_program("lldb-vscode", path_env))
        .or_else(|| {
            [
                "/opt/homebrew/opt/llvm/bin/lldb-dap",
                "/usr/local/opt/llvm/bin/lldb-dap",
            ]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
        })
}

pub fn find_codelldb(path_env: &str) -> Option<PathBuf> {
    if let Some(found) = find_program("codelldb", path_env) {
        return Some(found);
    }
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
    let mut found: Vec<PathBuf> = std::fs::read_dir(home.join(".vscode").join("extensions"))
        .ok()?
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("vadimcn.vscode-lldb-")
        })
        .map(|e| e.path().join("adapter").join("codelldb"))
        .filter(|p| p.is_file())
        .collect();
    found.sort();
    found.pop()
}

/// The adapter for a configuration, ready to start: `debugAdapterPath`
/// (and `debugAdapterArgs`) when the configuration names one (a stdio
/// adapter, e.g. a custom lldb-dap build), else the one for its `type`.
pub fn adapter_for_config(
    raw: &Value,
    path_env: &str,
    servers: Option<&Path>,
    cwd: &Path,
) -> Result<(AdapterKind, AdapterCommand), Missing> {
    let kind = raw["type"].as_str().unwrap_or_default();
    if let Some(program) = raw["debugAdapterPath"].as_str().filter(|p| !p.is_empty()) {
        let adapter = AdapterKind::for_type(kind).unwrap_or(AdapterKind::LldbDap);
        let program = find_program(program, path_env).ok_or_else(|| Missing {
            adapter,
            hint: format!("debugAdapterPath `{program}` not found"),
            installable: false,
        })?;
        let args = raw["debugAdapterArgs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a.as_str().map(str::to_string))
            .collect();
        return Ok((
            adapter,
            AdapterCommand {
                name: program
                    .file_name()
                    .map_or_else(|| "adapter".into(), |n| n.to_string_lossy().to_string()),
                program,
                args,
                cwd: Some(cwd.to_path_buf()),
                path_env: Some(path_env.to_string()),
                transport: Transport::Stdio,
            },
        ));
    }
    adapter_command(kind, path_env, servers, cwd)
}

/// The adapter for a configuration `type`, ready to start. `path_env`
/// should include the editor's installed tools (`with_managed`).
pub fn adapter_command(
    kind: &str,
    path_env: &str,
    servers: Option<&Path>,
    cwd: &Path,
) -> Result<(AdapterKind, AdapterCommand), Missing> {
    let wanted = AdapterKind::for_type(kind).ok_or(Missing {
        adapter: AdapterKind::LldbDap,
        hint: format!("debug type `{kind}` is not supported"),
        installable: false,
    })?;
    let command = |name: &str, program: PathBuf, args: Vec<String>, transport| AdapterCommand {
        name: name.to_string(),
        program,
        args,
        cwd: Some(cwd.to_path_buf()),
        path_env: Some(path_env.to_string()),
        transport,
    };
    let codelldb = |program| {
        command(
            "CodeLLDB",
            program,
            vec!["--port".into(), "{port}".into()],
            Transport::Tcp,
        )
    };
    match wanted {
        AdapterKind::LldbDap => {
            if let Some(program) = find_lldb_dap(path_env) {
                return Ok((
                    wanted,
                    command("lldb-dap", program, vec![], Transport::Stdio),
                ));
            }
            if let Some(program) = find_codelldb(path_env) {
                return Ok((AdapterKind::CodeLldb, codelldb(program)));
            }
            Err(Missing {
                adapter: wanted,
                hint: if cfg!(target_os = "macos") {
                    "install the Xcode Command Line Tools (xcode-select --install)".into()
                } else {
                    "install LLVM's lldb-dap (e.g. apt install lldb)".into()
                },
                installable: false,
            })
        }
        AdapterKind::CodeLldb => match find_codelldb(path_env) {
            Some(program) => Ok((wanted, codelldb(program))),
            None => adapter_command("lldb", path_env, servers, cwd).map_err(|_| Missing {
                adapter: wanted,
                hint: "install the CodeLLDB VS Code extension or lldb-dap".into(),
                installable: false,
            }),
        },
        AdapterKind::Delve => match find_program("dlv", path_env) {
            Some(program) => Ok((
                wanted,
                command(
                    "dlv dap",
                    program,
                    vec!["dap".into(), "--listen".into(), "127.0.0.1:{port}".into()],
                    Transport::Tcp,
                ),
            )),
            None => Err(Missing {
                adapter: wanted,
                hint: "go install github.com/go-delve/delve/cmd/dlv@latest".into(),
                installable: true,
            }),
        },
        AdapterKind::JsDebug => {
            let server = servers.map(js_debug_server).filter(|p| p.is_file());
            let node = find_program("node", path_env);
            match (server, node) {
                (Some(server), Some(node)) => Ok((
                    wanted,
                    command(
                        "js-debug",
                        node,
                        vec![server.to_string_lossy().to_string(), "{port}".into(), "127.0.0.1".into()],
                        Transport::Tcp,
                    ),
                )),
                _ => Err(Missing {
                    adapter: wanted,
                    hint: "run \"Debug: Install Adapter\" (js-debug from github.com/microsoft/vscode-js-debug)".into(),
                    installable: servers.is_some(),
                }),
            }
        }
    }
}

/// The `launch` / `attach` arguments for `adapter`, from a configuration
/// (`program` already resolved for cargo builds).
pub fn launch_arguments(adapter: AdapterKind, raw: &Value) -> Value {
    let mut args = raw.clone();
    if let Some(map) = args.as_object_mut() {
        map.remove("cargo");
        map.remove("preLaunchTask");
        map.remove("postDebugTask");
        map.remove("debugAdapterPath");
        map.remove("debugAdapterArgs");
    }
    match adapter {
        AdapterKind::LldbDap => {
            // lldb-dap takes `env` as a list of `K=V` strings.
            if let Some(env) = raw["env"].as_object() {
                args["env"] = Value::Array(
                    env.iter()
                        .map(|(k, v)| json!(format!("{k}={}", v.as_str().unwrap_or_default())))
                        .collect(),
                );
            }
            if raw["pid"].is_null() && !raw["processId"].is_null() {
                args["pid"] = raw["processId"].clone();
            }
        }
        AdapterKind::JsDebug => {
            if raw["type"] == "node" {
                args["type"] = json!("pwa-node");
            }
            if args["console"].is_null() {
                args["console"] = json!("internalConsole");
            }
        }
        AdapterKind::CodeLldb | AdapterKind::Delve => {}
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_map_to_adapters() {
        assert_eq!(AdapterKind::for_type("lldb"), Some(AdapterKind::LldbDap));
        assert_eq!(
            AdapterKind::for_type("pwa-node"),
            Some(AdapterKind::JsDebug)
        );
        assert_eq!(AdapterKind::for_type("go"), Some(AdapterKind::Delve));
        assert_eq!(AdapterKind::for_type("python"), None);
    }

    #[test]
    fn missing_adapters_explain_themselves() {
        let empty = tempfile::tempdir().expect("tempdir");
        let path = empty.path().to_string_lossy().to_string();
        let go = adapter_command("go", &path, None, empty.path()).expect_err("no dlv");
        assert!(go.installable && go.hint.contains("go-delve"));
        let js = adapter_command("node", &path, Some(empty.path()), empty.path())
            .expect_err("no js-debug");
        assert!(js.installable);
        let bin = empty.path().join("dlv");
        std::fs::write(&bin, "#!/bin/sh\n").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        let (kind, command) = adapter_command("go", &path, None, empty.path()).expect("dlv");
        let custom = json!({"type": "lldb", "debugAdapterPath": bin.to_string_lossy(), "debugAdapterArgs": ["-v"]});
        let (_, own) = adapter_for_config(&custom, &path, None, empty.path()).expect("custom");
        assert_eq!(
            (own.program.as_path(), own.args.as_slice()),
            (bin.as_path(), &["-v".to_string()][..])
        );
        assert!(
            launch_arguments(AdapterKind::LldbDap, &custom)
                .get("debugAdapterPath")
                .is_none()
        );
        assert_eq!(kind, AdapterKind::Delve);
        assert_eq!(command.transport, Transport::Tcp);
        assert_eq!(command.args, ["dap", "--listen", "127.0.0.1:{port}"]);
    }

    #[test]
    fn arguments_per_adapter() {
        let raw = json!({"type": "lldb", "program": "/a", "env": {"RUST_LOG": "debug"}, "cargo": {"args": ["build"]}});
        let args = launch_arguments(AdapterKind::LldbDap, &raw);
        assert_eq!(args["env"], json!(["RUST_LOG=debug"]));
        assert!(args.get("cargo").is_none());
        let node = launch_arguments(
            AdapterKind::JsDebug,
            &json!({"type": "node", "program": "a.js"}),
        );
        assert_eq!(node["type"], "pwa-node");
        assert_eq!(node["console"], "internalConsole");
    }
}
