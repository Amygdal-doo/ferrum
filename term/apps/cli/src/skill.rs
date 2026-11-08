//! Agent integration: a skill that teaches Claude Code / Codex to drive
//! FerrumTerm, plus optional Claude Code hooks that report the agent's status
//! (working → done / blocked) so you see it in tabs, `fterm ls` and notifications.

use clap::Subcommand;
use std::path::PathBuf;
use std::process::ExitCode;

const SKILL: &str = r#"---
name: fterm
description: Control FerrumTerm terminals with the `fterm` CLI — open tabs and split panes, run dev servers or tests in the background, read another terminal's screen, type into it, wait for programs to finish, and report your own status. Use whenever the user mentions panes, splits, tabs, other terminals, background processes, or FerrumTerm.
---

# FerrumTerm (`fterm`)

FerrumTerm is a persistent terminal multiplexer. Every terminal ("block") lives in a
server, so programs keep running when windows close. You are probably running inside
one: `$FTERM_BLOCK` (e.g. `b4`) is your block, `$FTERM_SESSION` your session.
Commands default to your own block, so "beside you" needs no target.

Targets: ids (`s1` session, `w2` window/tab, `b3` block), a session name, a window
name, or a block label (`--label`).

## Look around
- `fterm ls` — sessions › windows › blocks, with program status (◐ working, ⚠ blocked, ✓ done, ✕ error).
- `fterm capture -t b3 -n 40` — last 40 lines of a block's screen; `-a` includes scrollback.
- `fterm inspect b3` — JSON details (command, cwd, size, status, exit code).

## Start things
- `fterm run --split right -l server -- npm run dev` — dev server in a split beside you (prints `s1 w1 b5`).
- `fterm run --split down --ratio 0.3 -- cargo test` — tests below you, 30% height.
- `fterm run --window -l logs -- tail -f app.log` — a new tab.
- `fterm run --session -l build -- make` — a separate session.
- `fterm run --wait -- cargo build` — run and wait; the exit code is fterm's exit code.
- Add `--keep-open` to keep the block (and its final screen) after the program exits.

## Drive and wait
- `fterm send -t server -e "npm run migrate"` — type text and press Enter.
- `fterm send-key -t b5 C-c` — keys: Enter, Tab, Escape, Up, Down, C-c, C-d, M-f, F5…
- `fterm wait -t b5 --timeout 10m` — until it exits (exit code passed through).
- `fterm wait -t b5 --status done,error,blocked` — until it reports a status.
- `fterm kill b5` — close a block (or `w2`, or a session).

## Report your status (so the user sees it, even with the window closed)
- `fterm status working -m "Running migrations"`; `fterm status blocked -k question -m "Which DB?"`;
  `fterm status done -m "Tests green"`; `fterm status clear`.

## Tips
- Prefer `capture -n N` over full captures to save tokens.
- Poll with `wait`, not sleep loops.
- Never kill blocks you did not start unless asked.
"#;

#[derive(Subcommand)]
pub enum SkillCmd {
    /// Install the skill for Claude Code (~/.claude/skills/fterm) and Codex (~/.agents/skills/fterm).
    Setup {
        /// Also add Claude Code hooks that report working / done / blocked status.
        #[arg(long)]
        hooks: bool,
    },
    /// Print the skill.
    Show,
    /// Show where it is installed.
    Status,
    /// Remove the skill (and hooks added by `setup --hooks`).
    Remove,
}

fn home() -> Result<PathBuf, String> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| "HOME is not set".to_owned())
}

fn targets() -> Result<Vec<PathBuf>, String> {
    let h = home()?;
    Ok(vec![
        h.join(".claude/skills/fterm/SKILL.md"),
        h.join(".agents/skills/fterm/SKILL.md"),
    ])
}

/// Hook commands are no-ops outside FerrumTerm.
fn hook(cmd: &str) -> serde_json::Value {
    serde_json::json!({ "hooks": [{ "type": "command", "command": format!("[ -n \"$FTERM_BLOCK\" ] && fterm {cmd} >/dev/null 2>&1 || true") }] })
}

const HOOK_EVENTS: [(&str, &str); 3] = [
    ("UserPromptSubmit", "status working"),
    ("Stop", "status done"),
    ("Notification", "status blocked -k permission"),
];

fn is_ours(v: &serde_json::Value) -> bool {
    v.to_string().contains("fterm status")
}

fn edit_claude_settings(add: bool) -> Result<bool, String> {
    let path = home()?.join(".claude/settings.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".into());
    let mut root: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let Some(obj) = root.as_object_mut() else {
        return Err(format!("{} is not a JSON object", path.display()));
    };
    let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let Some(hooks) = hooks.as_object_mut() else {
        return Err("`hooks` in settings.json is not an object".into());
    };
    let mut changed = false;
    for (event, cmd) in HOOK_EVENTS {
        let list = hooks.entry(event).or_insert_with(|| serde_json::json!([]));
        let Some(list) = list.as_array_mut() else {
            continue;
        };
        let before = list.len();
        list.retain(|v| !is_ours(v));
        if add {
            list.push(hook(cmd));
        }
        changed |= add || list.len() != before;
    }
    hooks.retain(|_, v| v.as_array().is_none_or(|a| !a.is_empty()));
    if changed {
        if path.exists() {
            std::fs::copy(&path, path.with_extension("json.fterm-backup"))
                .map_err(|e| e.to_string())?;
        }
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n",
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(changed)
}

pub fn run(cmd: SkillCmd) -> Result<ExitCode, String> {
    match cmd {
        SkillCmd::Setup { hooks } => {
            for t in targets()? {
                if let Some(d) = t.parent() {
                    std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
                }
                std::fs::write(&t, SKILL).map_err(|e| e.to_string())?;
                println!("Installed {}", t.display());
            }
            if hooks {
                edit_claude_settings(true)?;
                println!(
                    "Added FerrumTerm status hooks to ~/.claude/settings.json (backup: settings.json.fterm-backup)"
                );
            } else {
                println!(
                    "Tip: `fterm skill setup --hooks` also reports Claude Code's working/done/blocked status to FerrumTerm."
                );
            }
        }
        SkillCmd::Show => print!("{SKILL}"),
        SkillCmd::Status => {
            for t in targets()? {
                println!(
                    "{} {}",
                    if t.exists() { "installed" } else { "missing  " },
                    t.display()
                );
            }
        }
        SkillCmd::Remove => {
            for t in targets()? {
                if let Some(d) = t.parent() {
                    let _ = std::fs::remove_dir_all(d);
                }
            }
            if edit_claude_settings(false)? {
                println!("Removed FerrumTerm hooks from ~/.claude/settings.json");
            }
            println!("Removed the fterm skill");
        }
    }
    Ok(ExitCode::SUCCESS)
}
