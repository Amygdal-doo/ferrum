//! The built-in tools: read_file, list_dir, grep, edit_file, write_file,
//! delete_file, run_command, git_status, git_diff and write_plan. Paths go
//! through the [`Sandbox`](super::Sandbox); writes through the change set.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::{Access, ToolContext, ToolDef, ToolOutput};
use crate::provider::ToolSpec;

const READ_LIMIT: usize = 256 * 1024;
const READ_LINES: usize = 2000;
const LIST_LIMIT: usize = 500;
const GREP_LIMIT: usize = 200;
const DIFF_LIMIT: usize = 60 * 1024;
const COMMAND_TIMEOUT: u64 = 120;

fn def(name: &str, access: Access, description: &str, schema: Value) -> ToolDef {
    ToolDef {
        spec: ToolSpec {
            name: name.into(),
            description: description.into(),
            input_schema: schema,
        },
        access,
    }
}

pub fn defs() -> Vec<ToolDef> {
    vec![
        def(
            "read_file",
            Access::Read,
            "Read a text file in the project. Lines are numbered. Use offset/limit (1-based lines) for large files.",
            json!({"type": "object", "properties": {
                "path": {"type": "string", "description": "Path relative to the project root"},
                "offset": {"type": "integer", "minimum": 1},
                "limit": {"type": "integer", "minimum": 1}
            }, "required": ["path"]}),
        ),
        def(
            "list_dir",
            Access::Read,
            "List a folder (gitignored files left out). Folders end with '/'.",
            json!({"type": "object", "properties": {
                "path": {"type": "string", "description": "Folder relative to the project root; default '.'"},
                "depth": {"type": "integer", "minimum": 1, "maximum": 4}
            }}),
        ),
        def(
            "grep",
            Access::Read,
            "Search file contents with a regular expression. Returns path:line: text.",
            json!({"type": "object", "properties": {
                "pattern": {"type": "string"},
                "path": {"type": "string", "description": "Folder or file to search; default '.'"},
                "glob": {"type": "string", "description": "Only files matching this glob, e.g. '*.rs'"},
                "case_insensitive": {"type": "boolean"}
            }, "required": ["pattern"]}),
        ),
        def(
            "edit_file",
            Access::Write,
            "Replace old_string with new_string in a file. old_string must match exactly once unless replace_all is true. Include enough context to make it unique.",
            json!({"type": "object", "properties": {
                "path": {"type": "string"},
                "old_string": {"type": "string"},
                "new_string": {"type": "string"},
                "replace_all": {"type": "boolean"}
            }, "required": ["path", "old_string", "new_string"]}),
        ),
        def(
            "write_file",
            Access::Write,
            "Create a file or replace its whole content.",
            json!({"type": "object", "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            }, "required": ["path", "content"]}),
        ),
        def(
            "delete_file",
            Access::Write,
            "Delete a file.",
            json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}),
        ),
        def(
            "run_command",
            Access::Execute,
            "Run a shell command in the project root and return its output and exit code. Avoid long-running servers.",
            json!({"type": "object", "properties": {
                "command": {"type": "string"},
                "timeout_secs": {"type": "integer", "minimum": 1, "maximum": 1800}
            }, "required": ["command"]}),
        ),
        def(
            "git_status",
            Access::Read,
            "Show the git branch and changed files.",
            json!({"type": "object", "properties": {}}),
        ),
        def(
            "git_diff",
            Access::Read,
            "Show the git diff of the working tree (or the staged changes).",
            json!({"type": "object", "properties": {
                "path": {"type": "string"},
                "staged": {"type": "boolean"}
            }}),
        ),
        def(
            "write_plan",
            Access::Plan,
            "Save the implementation plan (markdown). The user reviews it and presses Build.",
            json!({"type": "object", "properties": {"markdown": {"type": "string"}}, "required": ["markdown"]}),
        ),
    ]
}

fn str_arg<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
    input[key]
        .as_str()
        .ok_or_else(|| format!("missing string argument `{key}`"))
}

/// Runs a built-in tool.
pub async fn run(ctx: &ToolContext, name: &str, input: Value) -> ToolOutput {
    let result = match name {
        "run_command" => return run_command(ctx, &input).await,
        "read_file" | "list_dir" | "grep" | "edit_file" | "write_file" | "delete_file"
        | "git_status" | "git_diff" => {
            let ctx = ctx.clone();
            let name = name.to_string();
            tokio::task::spawn_blocking(move || blocking(&ctx, &name, &input))
                .await
                .unwrap_or_else(|e| Err(format!("tool panicked: {e}")))
        }
        "write_plan" => str_arg(&input, "markdown").map(|_| ToolOutput::ok("Plan saved.")),
        other => Err(format!("unknown tool `{other}`")),
    };
    result.unwrap_or_else(ToolOutput::error)
}

fn blocking(ctx: &ToolContext, name: &str, input: &Value) -> Result<ToolOutput, String> {
    match name {
        "read_file" => read_file(ctx, input),
        "list_dir" => list_dir(ctx, input),
        "grep" => grep(ctx, input),
        "edit_file" => edit_file(ctx, input),
        "write_file" => write_file(ctx, input),
        "delete_file" => delete_file(ctx, input),
        "git_status" => git(ctx, &["status", "--porcelain=v1", "-b"]),
        "git_diff" => {
            let mut args = vec!["diff", "--no-color"];
            if input["staged"].as_bool() == Some(true) {
                args.push("--cached");
            }
            let path = match input["path"].as_str() {
                Some(p) => Some(
                    ctx.sandbox
                        .display(&ctx.sandbox.resolve(p).map_err(|e| e.to_string())?),
                ),
                None => None,
            };
            if let Some(path) = &path {
                args.push("--");
                args.push(path);
            }
            git(ctx, &args).map(|mut out| {
                if out.text.trim().is_empty() {
                    out.text = "No changes.".into();
                }
                out.text = cap(&out.text, DIFF_LIMIT);
                out
            })
        }
        other => Err(format!("unknown tool `{other}`")),
    }
}

fn cap(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…(truncated)", &text[..end])
}

fn read_text(path: &Path, shown: &str) -> Result<String, String> {
    if path.is_dir() {
        return Err(format!("{shown} is a folder; use list_dir"));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{shown}: {e}"))?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(format!("{shown} is a binary file"));
    }
    String::from_utf8(bytes).map_err(|_| format!("{shown} is not UTF-8 text"))
}

fn read_file(ctx: &ToolContext, input: &Value) -> Result<ToolOutput, String> {
    let shown = str_arg(input, "path")?;
    let path = ctx.sandbox.resolve(shown).map_err(|e| e.to_string())?;
    let text = read_text(&path, shown)?;
    let offset = input["offset"].as_u64().unwrap_or(1).max(1) as usize;
    let limit = input["limit"].as_u64().map_or(READ_LINES, |l| l as usize);
    let total = text.lines().count();
    let mut out = String::new();
    for (i, line) in text.lines().enumerate().skip(offset - 1).take(limit) {
        if out.len() > READ_LIMIT {
            out.push_str("…(truncated; use offset/limit)\n");
            break;
        }
        out.push_str(&format!("{:>5}\t{line}\n", i + 1));
    }
    let end = (offset - 1 + limit).min(total);
    if end < total {
        out.push_str(&format!(
            "…({} more lines; read with offset={})\n",
            total - end,
            end + 1
        ));
    }
    if text.is_empty() {
        out.push_str("(empty file)");
    }
    Ok(ToolOutput::ok(out))
}

fn walker(root: &Path, depth: Option<usize>) -> ignore::Walk {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .max_depth(depth)
        .filter_entry(|e| e.file_name() != ".git")
        .sort_by_file_name(|a, b| a.cmp(b))
        .build()
}

fn list_dir(ctx: &ToolContext, input: &Value) -> Result<ToolOutput, String> {
    let shown = input["path"].as_str().unwrap_or(".");
    let path = ctx.sandbox.resolve(shown).map_err(|e| e.to_string())?;
    if !path.is_dir() {
        return Err(format!("{shown} is not a folder"));
    }
    let depth = input["depth"].as_u64().unwrap_or(1).clamp(1, 4) as usize;
    let mut out = String::new();
    let mut count = 0;
    for entry in walker(&path, Some(depth)).flatten() {
        if entry.depth() == 0 {
            continue;
        }
        if count >= LIST_LIMIT {
            out.push_str("…(more entries)\n");
            break;
        }
        let rel = entry.path().strip_prefix(&path).unwrap_or(entry.path());
        let dir = entry.file_type().is_some_and(|t| t.is_dir());
        out.push_str(&format!(
            "{}{}\n",
            rel.to_string_lossy().replace('\\', "/"),
            if dir { "/" } else { "" }
        ));
        count += 1;
    }
    if out.is_empty() {
        out.push_str("(empty folder)");
    }
    Ok(ToolOutput::ok(out))
}

fn grep(ctx: &ToolContext, input: &Value) -> Result<ToolOutput, String> {
    let pattern = str_arg(input, "pattern")?;
    let regex = regex::RegexBuilder::new(pattern)
        .case_insensitive(input["case_insensitive"].as_bool() == Some(true))
        .size_limit(1 << 20)
        .build()
        .map_err(|e| format!("bad pattern: {e}"))?;
    let shown = input["path"].as_str().unwrap_or(".");
    let path = ctx.sandbox.resolve(shown).map_err(|e| e.to_string())?;
    let glob = match input["glob"].as_str().filter(|g| !g.is_empty()) {
        Some(g) => Some(
            globset::GlobBuilder::new(g)
                .literal_separator(false)
                .build()
                .map_err(|e| format!("bad glob: {e}"))?
                .compile_matcher(),
        ),
        None => None,
    };
    let mut out = String::new();
    let mut hits = 0;
    'files: for entry in walker(&path, None).flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let rel = ctx.sandbox.display(entry.path());
        if let Some(glob) = &glob {
            let name = entry.file_name().to_string_lossy();
            if !glob.is_match(&rel) && !glob.is_match(name.as_ref()) {
                continue;
            }
        }
        if entry.metadata().map_or(true, |m| m.len() > 2 * 1024 * 1024) {
            continue;
        }
        let Ok(text) = read_text(entry.path(), &rel) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                let line: String = line.chars().take(300).collect();
                out.push_str(&format!("{rel}:{}: {}\n", i + 1, line.trim_end()));
                hits += 1;
                if hits >= GREP_LIMIT {
                    out.push_str("…(more matches; narrow the search)\n");
                    break 'files;
                }
            }
        }
    }
    if hits == 0 {
        out.push_str("No matches.");
    }
    Ok(ToolOutput::ok(out))
}

fn write(ctx: &ToolContext, path: &Path, content: Option<&[u8]>) -> Result<(), String> {
    let rel = ctx.sandbox.relative(path);
    let mut changes = ctx
        .changes
        .lock()
        .map_err(|_| "the change set is unavailable".to_string())?;
    changes.write(&rel, content).map_err(|e| e.to_string())
}

fn edit_file(ctx: &ToolContext, input: &Value) -> Result<ToolOutput, String> {
    let shown = str_arg(input, "path")?;
    let old = str_arg(input, "old_string")?;
    let new = str_arg(input, "new_string")?;
    let path = ctx
        .sandbox
        .resolve_for_write(shown)
        .map_err(|e| e.to_string())?;
    if !path.exists() {
        return Err(format!(
            "{shown} does not exist; use write_file to create it"
        ));
    }
    if old.is_empty() {
        return Err("old_string is empty; use write_file to replace a whole file".into());
    }
    let text = read_text(&path, shown)?;
    let count = text.matches(old).count();
    let all = input["replace_all"].as_bool() == Some(true);
    let updated = match count {
        0 => return Err(format!("old_string was not found in {shown}")),
        1 => text.replacen(old, new, 1),
        n if all => {
            let _ = n;
            text.replace(old, new)
        }
        n => {
            return Err(format!(
                "old_string matches {n} times in {shown}; add context or set replace_all"
            ));
        }
    };
    write(ctx, &path, Some(updated.as_bytes()))?;
    let mut out = ToolOutput::ok(format!(
        "Edited {} ({} replacement{}).",
        ctx.sandbox.display(&path),
        if all { count } else { 1 },
        if all && count != 1 { "s" } else { "" }
    ));
    out.changed.push(ctx.sandbox.relative(&path));
    Ok(out)
}

fn write_file(ctx: &ToolContext, input: &Value) -> Result<ToolOutput, String> {
    let shown = str_arg(input, "path")?;
    let content = str_arg(input, "content")?;
    let path = ctx
        .sandbox
        .resolve_for_write(shown)
        .map_err(|e| e.to_string())?;
    if path.is_dir() {
        return Err(format!("{shown} is a folder"));
    }
    let existed = path.exists();
    write(ctx, &path, Some(content.as_bytes()))?;
    let mut out = ToolOutput::ok(format!(
        "{} {} ({} lines).",
        if existed { "Wrote" } else { "Created" },
        ctx.sandbox.display(&path),
        content.lines().count()
    ));
    out.changed.push(ctx.sandbox.relative(&path));
    Ok(out)
}

fn delete_file(ctx: &ToolContext, input: &Value) -> Result<ToolOutput, String> {
    let shown = str_arg(input, "path")?;
    let path = ctx
        .sandbox
        .resolve_for_write(shown)
        .map_err(|e| e.to_string())?;
    if !path.is_file() {
        return Err(format!("{shown} is not a file"));
    }
    write(ctx, &path, None)?;
    let mut out = ToolOutput::ok(format!("Deleted {}.", ctx.sandbox.display(&path)));
    out.changed.push(ctx.sandbox.relative(&path));
    Ok(out)
}

fn git(ctx: &ToolContext, args: &[&str]) -> Result<ToolOutput, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(ctx.sandbox.root())
        .stdin(std::process::Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(ToolOutput::ok(String::from_utf8_lossy(&output.stdout)))
}

async fn run_command(ctx: &ToolContext, input: &Value) -> ToolOutput {
    let command = match str_arg(input, "command") {
        Ok(c) if !c.trim().is_empty() => c,
        Ok(_) => return ToolOutput::error("command is empty"),
        Err(e) => return ToolOutput::error(e),
    };
    let timeout = input["timeout_secs"]
        .as_u64()
        .unwrap_or(COMMAND_TIMEOUT)
        .clamp(1, 1800);
    let result = ctx
        .runner
        .run(command, ctx.sandbox.root(), Duration::from_secs(timeout))
        .await;
    let status = match (result.timed_out, result.exit_code) {
        (true, _) => "timed out".to_string(),
        (false, Some(code)) => format!("exit code {code}"),
        (false, None) => "terminated".to_string(),
    };
    ToolOutput {
        text: format!("{status}\n{}", result.output),
        is_error: result.timed_out || result.exit_code != Some(0),
        changed: Vec::new(),
    }
}
