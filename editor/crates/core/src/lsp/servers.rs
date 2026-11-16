//! Which language server runs for which language, how to find its program
//! and which folder it is started in. Users override the command per
//! server id in `config.toml`:
//!
//! ```toml
//! [lsp.python]
//! command = "pylsp"
//! args = []
//! ```

use std::path::{Path, PathBuf};

use crate::config::{LspConfig, LspServerConfig};
use crate::language::Language;

#[derive(Debug)]
pub struct ServerDef {
    /// Id used in `[lsp.<id>]`.
    pub id: &'static str,
    pub languages: &'static [Language],
    /// Programs tried in order, with their arguments.
    pub commands: &'static [(&'static str, &'static [&'static str])],
    /// Files marking a project root, nearest first.
    pub root_markers: &'static [&'static str],
    /// How to install the default program.
    pub install: &'static str,
}

pub const SERVERS: &[ServerDef] = &[
    ServerDef {
        id: "rust",
        languages: &[Language::Rust],
        commands: &[("rust-analyzer", &[])],
        root_markers: &["Cargo.toml"],
        install: "rustup component add rust-analyzer",
    },
    ServerDef {
        id: "typescript",
        languages: &[Language::JavaScript, Language::TypeScript, Language::Tsx],
        commands: &[("typescript-language-server", &["--stdio"])],
        root_markers: &["tsconfig.json", "jsconfig.json", "package.json"],
        install: "npm install -g typescript@5 typescript-language-server",
    },
    ServerDef {
        id: "python",
        languages: &[Language::Python],
        commands: &[
            ("pyright-langserver", &["--stdio"]),
            ("basedpyright-langserver", &["--stdio"]),
            ("pylsp", &[]),
        ],
        root_markers: &["pyproject.toml", "setup.py", "requirements.txt"],
        install: "npm install -g pyright",
    },
    ServerDef {
        id: "go",
        languages: &[Language::Go],
        commands: &[("gopls", &[])],
        root_markers: &["go.work", "go.mod"],
        install: "go install golang.org/x/tools/gopls@latest",
    },
    ServerDef {
        id: "json",
        languages: &[Language::Json],
        commands: &[("vscode-json-language-server", &["--stdio"])],
        root_markers: &["package.json"],
        install: "npm install -g vscode-langservers-extracted",
    },
    ServerDef {
        id: "css",
        languages: &[Language::Css],
        commands: &[("vscode-css-language-server", &["--stdio"])],
        root_markers: &["package.json"],
        install: "npm install -g vscode-langservers-extracted",
    },
    ServerDef {
        id: "html",
        languages: &[Language::Html],
        commands: &[("vscode-html-language-server", &["--stdio"])],
        root_markers: &["package.json"],
        install: "npm install -g vscode-langservers-extracted",
    },
    ServerDef {
        id: "toml",
        languages: &[Language::Toml],
        commands: &[("taplo", &["lsp", "stdio"])],
        root_markers: &["Cargo.toml", "pyproject.toml"],
        install: "cargo install taplo-cli --locked --features lsp",
    },
    ServerDef {
        id: "yaml",
        languages: &[Language::Yaml],
        commands: &[("yaml-language-server", &["--stdio"])],
        root_markers: &[],
        install: "npm install -g yaml-language-server",
    },
    ServerDef {
        id: "shell",
        languages: &[Language::Bash],
        commands: &[("bash-language-server", &["start"])],
        root_markers: &[],
        install: "npm install -g bash-language-server",
    },
];

pub fn server_for(language: Language) -> Option<&'static ServerDef> {
    SERVERS.iter().find(|s| s.languages.contains(&language))
}

pub fn server_by_id(id: &str) -> Option<&'static ServerDef> {
    SERVERS.iter().find(|s| s.id == id)
}

/// The `languageId` sent in `didOpen`.
pub fn language_id(language: Language, path: &Path) -> &'static str {
    let jsx = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("jsx"));
    match language {
        Language::JavaScript if jsx => "javascriptreact",
        Language::Tsx => "typescriptreact",
        Language::Bash => "shellscript",
        Language::PlainText => "plaintext",
        other => other.id(),
    }
}

/// A resolved command, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Found {
        program: PathBuf,
        args: Vec<String>,
    },
    Disabled,
    /// No program found; `tried` lists the names looked for.
    Missing {
        tried: Vec<String>,
    },
}

/// Finds the program for `def`, honouring `[lsp.<id>]` overrides.
pub fn resolve(def: &ServerDef, config: &LspConfig, path_env: &str) -> Resolved {
    let user: Option<&LspServerConfig> = config.servers.get(def.id);
    if !config.enabled || user.and_then(|u| u.enabled) == Some(false) {
        return Resolved::Disabled;
    }
    if let Some(command) = user
        .and_then(|u| u.command.as_deref())
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        let args = user.and_then(|u| u.args.clone()).unwrap_or_default();
        return match find_program(command, path_env) {
            Some(program) => Resolved::Found { program, args },
            None => Resolved::Missing {
                tried: vec![command.to_string()],
            },
        };
    }
    for (name, args) in def.commands {
        if let Some(program) = find_program(name, path_env) {
            let args = user
                .and_then(|u| u.args.clone())
                .unwrap_or_else(|| args.iter().map(|a| (*a).to_string()).collect());
            return Resolved::Found { program, args };
        }
    }
    Resolved::Missing {
        tried: def.commands.iter().map(|(n, _)| (*n).to_string()).collect(),
    }
}

/// Looks `name` up in `path_env` (absolute paths and `~/` are used as-is).
pub fn find_program(name: &str, path_env: &str) -> Option<PathBuf> {
    let expanded = expand_home(name);
    if expanded.components().count() > 1 {
        return is_executable(&expanded).then_some(expanded);
    }
    std::env::split_paths(path_env)
        .flat_map(|dir| {
            let plain = dir.join(name);
            let exe = cfg!(windows).then(|| dir.join(format!("{name}.exe")));
            let cmd = cfg!(windows).then(|| dir.join(format!("{name}.cmd")));
            [Some(plain), exe, cmd].into_iter().flatten()
        })
        .find(|candidate| is_executable(candidate))
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// `PATH` for finding and running servers: the login shell's (apps
/// started from the Dock don't get it), plus the usual tool folders.
pub fn search_path(login_shell_path: Option<&str>) -> String {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for source in [login_shell_path, std::env::var("PATH").ok().as_deref()]
        .into_iter()
        .flatten()
    {
        dirs.extend(std::env::split_paths(source));
    }
    if let Some(home) = home_dir() {
        for extra in [
            ".cargo/bin",
            ".local/bin",
            "go/bin",
            ".bun/bin",
            ".deno/bin",
        ] {
            dirs.push(home.join(extra));
        }
    }
    for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        dirs.push(PathBuf::from(extra));
    }
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| !d.as_os_str().is_empty() && seen.insert(d.clone()));
    std::env::join_paths(dirs)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// `path_env` with the project's `node_modules/.bin` folders (from `file`'s
/// folder up to `root`) first, so a project's own TypeScript tooling wins.
pub fn with_project_bins(path_env: &str, root: &Path, file: &Path) -> String {
    let mut dirs: Vec<PathBuf> = file
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .take_while(|dir| dir.starts_with(root))
        .map(|dir| dir.join("node_modules").join(".bin"))
        .filter(|dir| dir.is_dir())
        .collect();
    if dirs.is_empty() {
        return path_env.to_string();
    }
    dirs.extend(std::env::split_paths(path_env));
    std::env::join_paths(dirs)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path_env.to_string())
}

/// Asks the user's login shell for its `PATH` (a few seconds at most).
pub fn login_shell_path() -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty())?;
    let mut child = std::process::Command::new(shell)
        .args([
            "-l",
            "-i",
            "-c",
            "printf '\\n__FERRUM_PATH__%s\\n' \"$PATH\"",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                child.kill().ok();
                child.wait().ok();
                return None;
            }
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    out.lines()
        .find_map(|l| l.strip_prefix("__FERRUM_PATH__"))
        .map(str::to_string)
        .filter(|p| !p.is_empty())
}

/// Folder a server for `file` is started in: the open project if the file
/// is inside it, else the nearest folder with a root marker, else the
/// file's folder.
pub fn root_for(def: &ServerDef, file: &Path, project: Option<&Path>) -> PathBuf {
    if let Some(project) = project
        && file.starts_with(project)
    {
        return project.to_path_buf();
    }
    let parent = file.parent().unwrap_or(file);
    parent
        .ancestors()
        .find(|dir| def.root_markers.iter().any(|m| dir.join(m).exists()))
        .unwrap_or(parent)
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn executable(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\n").expect("test value");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("test value");
        }
        path
    }

    #[test]
    fn every_language_with_a_grammar_but_text_and_markdown_has_a_server() {
        for language in Language::ALL {
            let expected = !matches!(language, Language::PlainText | Language::Markdown);
            assert_eq!(server_for(language).is_some(), expected, "{language:?}");
        }
        assert_eq!(
            server_by_id("rust").expect("test value").commands[0].0,
            "rust-analyzer"
        );
    }

    #[test]
    fn resolves_defaults_fallbacks_and_overrides() {
        let dir = tempfile::tempdir().expect("test value");
        let path_env = dir.path().to_string_lossy().to_string();
        let python = server_by_id("python").expect("test value");
        let config = LspConfig::default();
        assert!(matches!(
            resolve(python, &config, &path_env),
            Resolved::Missing { tried } if tried.len() == 3
        ));
        let pylsp = executable(dir.path(), "pylsp");
        assert_eq!(
            resolve(python, &config, &path_env),
            Resolved::Found {
                program: pylsp,
                args: vec![]
            }
        );
        let custom = executable(dir.path(), "my-server");
        let mut servers = BTreeMap::new();
        servers.insert(
            "python".to_string(),
            LspServerConfig {
                command: Some("my-server".into()),
                args: Some(vec!["--fast".into()]),
                ..Default::default()
            },
        );
        let config = LspConfig {
            servers,
            ..Default::default()
        };
        assert_eq!(
            resolve(python, &config, &path_env),
            Resolved::Found {
                program: custom,
                args: vec!["--fast".into()]
            }
        );
        let off = LspConfig {
            enabled: false,
            ..Default::default()
        };
        assert_eq!(resolve(python, &off, &path_env), Resolved::Disabled);
    }

    #[test]
    fn roots_prefer_the_project_then_markers() {
        let dir = tempfile::tempdir().expect("test value");
        let crate_dir = dir.path().join("lib");
        std::fs::create_dir_all(crate_dir.join("src")).expect("test value");
        std::fs::write(crate_dir.join("Cargo.toml"), "").expect("test value");
        let file = crate_dir.join("src/lib.rs");
        let rust = server_by_id("rust").expect("test value");
        assert_eq!(root_for(rust, &file, Some(dir.path())), dir.path());
        assert_eq!(root_for(rust, &file, None), crate_dir);
        let yaml = server_by_id("yaml").expect("test value");
        assert_eq!(root_for(yaml, &file, None), crate_dir.join("src"));
    }

    #[test]
    fn project_node_bins_come_first() {
        let dir = tempfile::tempdir().expect("test value");
        let root = dir.path();
        let web = root.join("web");
        std::fs::create_dir_all(web.join("node_modules/.bin")).expect("test value");
        std::fs::create_dir_all(root.join("node_modules/.bin")).expect("test value");
        let file = web.join("src/app.ts");
        let path = with_project_bins("/usr/bin", root, &file);
        let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
        assert_eq!(dirs[0], web.join("node_modules/.bin"));
        assert_eq!(dirs[1], root.join("node_modules/.bin"));
        assert_eq!(dirs[2], PathBuf::from("/usr/bin"));
        assert_eq!(
            with_project_bins("/usr/bin", &web, &root.join("x.ts")),
            "/usr/bin"
        );
    }

    #[test]
    fn language_ids() {
        assert_eq!(
            language_id(Language::Tsx, Path::new("a.tsx")),
            "typescriptreact"
        );
        assert_eq!(
            language_id(Language::JavaScript, Path::new("a.jsx")),
            "javascriptreact"
        );
        assert_eq!(
            language_id(Language::Bash, Path::new("a.sh")),
            "shellscript"
        );
        assert_eq!(language_id(Language::Rust, Path::new("a.rs")), "rust");
    }

    #[test]
    fn search_path_adds_tool_folders_once() {
        let path = search_path(Some("/opt/homebrew/bin:/custom/bin"));
        let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
        assert_eq!(dirs[0], PathBuf::from("/opt/homebrew/bin"));
        assert!(dirs.contains(&PathBuf::from("/custom/bin")));
        let homebrew = dirs
            .iter()
            .filter(|d| *d == Path::new("/opt/homebrew/bin"))
            .count();
        assert_eq!(homebrew, 1);
    }
}
