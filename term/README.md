# FerrumTerm

A Rust terminal for humans and AI agents, inspired by Ghostty (GPU-rendered window), tmux
(persistent sessions, attach from anywhere) and Rex (agent status, scriptable CLI,
Tailscale access). Every line is Rust: `alacritty_terminal` for VT emulation,
`portable-pty`, Tokio, GPUI (via the shared `ferrum-ui` crate, same UI stack as the other Ferrum apps) for
the window, crossterm for `attach`.

> **Status: M0, first vertical slice.** The server, CLI and attach are covered by
> automated tests (real PTYs, no network). The window was ported from winit + wgpu + glyphon to
> GPUI; the GPUI version has been launch-tested (connects, creates a session, sizes the grid) but
> not yet visually re-checked or used day-to-day by a person.

## The idea: terminals live in a server
```
FerrumTerm window ─┐                      ┌─ zsh            (b3)
fterm CLI / agents ─┼─ Unix socket ─► fterm server ─┼─ claude code   (b4)  ◐ working
fterm attach (ssh) ─┤  (or tailnet TCP + token)     └─ npm run dev   (b5)
other Mac ─────────┘
```
Closing the window only disconnects a client. Programs — agents included — keep running in the
server, and so does agent status detection; with no window open the server posts the
notifications. Quitting FerrumTerm doesn't stop the server either: only `fterm server stop`
(or logging out / rebooting) ends the terminals. You can reopen
FerrumTerm, `fterm attach` over SSH, or reach the server from another Mac on your tailnet.

**After an update the old server keeps running** (like tmux), so it runs the old build's code
until restarted. Every server reports its build: the release version (`FERRUM_VERSION` at build
time, else the crate version) and a *server revision* (`SERVER_REVISION` in
`crates/protocol/src/version.rs`, bumped whenever server behaviour clients rely on changes, e.g.
agent detection or notifications). Both are in the handshake (`Welcome`) and in
`fterm server status`. Servers from before 0.12 report neither and count as older.
- The FerrumTerm window and the embedded panes (FerrumIDE, FerrumEngineer) show a notice when
  the shared server has a lower revision: *"The FerrumTerm server is older than this app (server
  X, app Y). Agent status and some features need a restart."* **Restart now** first warns
  *"Restarting ends the programs running in all terminals (N sessions, M terminals), in every app
  using this server."*, then stops it and starts the current build (the `fterm` next to the app,
  else the installed FerrumTerm.app's in `/Applications[/Ferrum]` or `~/Applications[/Ferrum]`,
  else `PATH`) and reconnects.
  **Later** hides it until the app quits. If the server is still older after a restart, the
  notice says the `fterm` that starts it is out of date.
- A server with the same revision but an older release version is not flagged in the apps (nothing
  they need is missing); `fterm server status` mentions it.
- `fterm server status` adds a line such as *"older than this fterm (0.12.0-alpha.1 (revision 1));
  agent status and some features need a restart: run `fterm server restart`"*, and warns when the
  LaunchAgent (`fterm server install`) starts a binary other than this `fterm`, or one that no
  longer exists (re-run `fterm server install`). `--json` adds `age`, `fterm_version`,
  `fterm_revision` and `launch_agent`.
- `fterm server restart [--yes]` stops the server (even one too old to talk to) and starts this
  `fterm`'s build with the same TCP listeners and keep-awake mode. It asks first, saying how many
  sessions and terminals will end, unless `--yes` (required when stdin is not a terminal).

**Agents with the display off:** while any block reports `working`, the server holds
`caffeinate -i -s` (no idle sleep, and no system sleep on AC power), so agents keep
working with the screen off or locked. *A MacBook with its lid closed still sleeps*
unless it is in clamshell mode (power + external display). macOS doesn't let a user process
override that. Use `fterm awake always` to hold the assertion permanently.

## What works today
| Area | State |
|------|-------|
| Persistent server: sessions → windows (tabs) → split blocks, PTYs, 10k-line scrollback | ✅ tested |
| `fterm server start` (detached, survives the terminal closing), `stop`, `restart`, `status` (build, revision, older-than-this-fterm and stale LaunchAgent warnings), `logs`, `install` (LaunchAgent: start at login, restart on crash) | ✅ start/stop/status tested; LaunchAgent written but not installed in testing |
| CLI like Rex's: `new ls run split window send send-key capture kill rename focus inspect wait status events awake`, `--json`, targets by id/name/label, `$FTERM_BLOCK` default | ✅ smoke-tested end to end |
| Program status: OSC 7501 (Rex-compatible) or `fterm status`; shown in `ls`, tabs and attach; `wait --status`; macOS notifications on done/blocked/error | ✅ tested |
| Keep-awake while agents work (`caffeinate`), `fterm awake auto/always/never` | ✅ implemented; manual check only |
| Agent skill for Claude Code (`~/.claude/skills/fterm`) and Codex (`~/.agents/skills/fterm`); `--hooks` adds Claude Code hooks for working/done/blocked status | ✅ implemented, not yet used with a live agent |
| tmux-style `fterm attach` (renders splits, status line, `C-b` prefix: d c % " o n p 0-9 x [ ]) | ✅ attach/detach checked in a real PTY |
| Remote: TCP listeners on loopback/Tailscale addresses only, 256-bit token, `--tailscale` auto-detects this Mac's tailnet IP, `fterm hosts add` + `fterm -S host …` | ✅ token auth tested on loopback; not tested across two machines |
| GPUI window: tabs, splits with focus accent, truecolor/256/16 colours, bold/italic/underline/strike/dim/inverse, block/beam/underline cursor, wide (CJK/emoji) cells, scrollback (wheel / Cmd+PgUp), paste (bracketed), copy screen, font size, IME composition | ⚠️ ported to GPUI; launch-tested, not yet visually re-checked |
| Sidebar (⌘B or the tab-bar button; open/closed and width remembered): **Agents** section listing every coding agent in any terminal (Claude Code, Codex, Gemini, aider, OpenCode, Amp, Goose, cursor-agent, Qwen, Crush — started from FerrumTerm *or typed into a shell*, `agent…` labels, or anything reporting a status), grouped with a count: **Needs you** (⚠ a question or permission prompt), **Done** (blue dot, red if it failed: finished a turn you haven't looked at; turns Idle once its terminal is on screen in the active window), **Working** (spinner), **Idle** (hollow circle). Each row: agent logo, tab name, muted subtitle (the agent's own title, else its directory), time since the last change (`<1m`, `12m`, `3h`, `1d`); click to show that terminal; the + next to the header is the New menu. Above it every session with its tabs and split panes, which can be grouped in **folders** (see below) | ⚠️ builds; grouping, subtitles and times unit-tested; not yet looked at on screen |
| **+ New agent** menu in the sidebar: Claude Code, Codex, recent custom commands, or "Custom command…" (inline field). Opens a new tab labelled after the program (`claude`, `claude-2`…), like `fterm run --window -l claude -- claude`, in the focused pane's directory, through your login shell so `.zprofile`/`.zshrc` PATH applies | ⚠️ builds; label/command logic unit-tested; not launched by hand |
| **Machines** in the sidebar: only the machines you added — hosts saved in FerrumTerm (name, user, host, port, identity file, jump host, startup command) and the `~/.ssh/config` hosts you added (wildcards skipped, `Include` followed, the file is never written). Existing files keep working: saved hosts, and config hosts that already had port forwards, stay listed; other config hosts are no longer shown automatically. **+ Add machine** opens "Add a machine": every ssh-config host (alias, user@hostname) with *Add* or *Added ✓*, and "Or type how ssh reaches it" (`user@host[:port]`, saved as a host named after it); Esc closes. Click a machine → new tab running the system `ssh` (`-p` / `-i` / `-J`, `-t host '<cmd>'` for a startup command; config hosts run `ssh <alias>`). Edit / delete in the host form (pencil on the row). No passwords or keys are stored: ssh / ssh-agent handle passphrases. The ssh connection belongs to the FerrumTerm server on this Mac, so it survives closing the window but not this Mac sleeping (run `tmux` on the machine for that; FerrumTerm installs nothing remotely). `fterm ssh <name>`, `fterm ssh --list [--json]` (all hosts, config included), `fterm ssh add <name> user@host [-p -i -J -c]`, `fterm ssh remove <name>` | ⚠️ machine list, add / rename / remove unit-tested; GUI builds, not looked at on screen; no real ssh session opened in testing |
| **Port forwards** per host (`L [bind:]port:host:port`, `R …`, `D [bind:]port`), edited in the host form; each row starts / stops on click with a green (running) / red (failed, with ssh's error) / grey dot. They run as `ssh -N -o ExitOnForwardFailure=yes -o BatchMode=yes` children **owned by the server**, so they survive closing the window, and stop with the server. A taken local port is refused up front, naming the process holding it. `fterm forward list [--json] / add <host> <spec> / remove <host> <id> / start <host>[/L8080] [spec] / stop <key\|host\|--all>` | ⚠️ start/stop/failure/port-taken covered by a server test with a stand-in command (no real ssh); BatchMode means the key must be in ssh-agent / keychain |
| **What's using port N?** Sidebar "What's using a port?" dialog (all listening ports, or one), and `fterm port <N> [--kill] [--json]`, `fterm ports [--json]`: PID, process, user, address, command line. macOS: `lsof +c 0 -nP -iTCP:<N> -sTCP:LISTEN` + `ps`; Linux: `ss -ltnp`, falling back to lsof. Kill asks first (SIGTERM), then asks again before SIGKILL; never automatic | ✅ lsof / ss / ps parsers tested on fixtures; a test finds its own listener; kill path not exercised on real processes |
| Mouse selection (drag inside a pane) and ⌘C copy; the text is captured on mouse-up, cleared by typing, paste or scrolling | ⚠️ selection/hit-testing unit-tested; not hand-tested |
| **Settings** (⌘, or the gear in the title row; shared `ferrum-ui` window): **Terminal** (font family and size, cursor style / blink / force style, Option key, copy on selection, activity in tabs, startup command, `fterm skill setup --hooks` button) saved in `~/.ferrum/ferrumterm/settings.json` and used by every terminal view, including the panes in FerrumIDE and FerrumEngineer; **Keyboard › Option key**: *Automatic* (Alt on US / ABC layouts, characters on others), *Alt*, *Types characters*, *Left Option is Alt*, *Right Option is Alt* (older `option_as_meta` files map to Alt / Types characters); **Machines**: this computer (is the FerrumTerm server running), each added machine with a reachable / unreachable dot (TCP connect to its ssh port every 30 s; machines behind a jump host aren't probed), Rename (a config host becomes a saved host reaching its alias) / Remove, *Add a machine* (same dialog), ssh-agent key count — no Update, as nothing runs remotely; **Notifications** (shared by every app): toast corner (top left / top right / bottom left / bottom right, default bottom right), master switches incl. *Play sounds* and volume, per event ("An agent needs you", "An agent finished", "An agent failed", "An agent started working") channels with a *Play* button (Ping, Glass, Basso). No telemetry page: Ferrum sends no usage data. First-launch onboarding with migration from iTerm2, Ghostty, Terminal.app, Warp and tmux through a coding agent | ⚠️ settings model, Option-key logic and parsing unit-tested; pages not yet looked at on screen after this change |
| **+ menu** (sidebar): Terminal; Agent (Claude Code, Codex, recent, custom); cloud agent (Claude Code / Codex on an SSH host via its login shell); saved tab configs (`name = command`, kept in `gui-state`); New worktree… (`git worktree add` to `<repo>-<branch>` beside the repo, new branch if needed, then a terminal in it); New session. Sidebar: terminals and SSH hosts on top, a resizable Agents section at the bottom (30% by default). An incompatible older server shows "Restart server" (and `fterm server stop` can stop it) | ⚠️ worktree creation and state parsing unit-tested; menu checked on screen; cloud agents not run against a real host |
| **Agent status without hooks** (server side, `crates/server/src/detect.rs`): once a second the server looks at every block whose *foreground process* is a known agent (`ps` of the terminal's foreground process group, also `node …/claude-code/cli.js`, `python -m aider`): a permission prompt / question near the bottom of the screen (`❯ 1. Yes`, `Allow …?`, `Do you want to proceed?`, `(y/n)`, `(Y)es/(N)o`) → *blocked* with the question as message; an "esc to interrupt" status line, a braille spinner in the title, or output in the last 2 s that wasn't the echo of typing / a resize → *working*; working followed by 3 s without any of that → *done*; otherwise *idle*. Statuses an agent reports itself (OSC 7501, `fterm status`, the Claude Code hooks) always win until another program takes the foreground. It runs in the server, so it keeps going with every window closed; `fterm wait --status`, keep-awake and `fterm ls` see it too | ✅ detection is pure functions with unit tests on Claude Code / Codex / Gemini / aider screen samples; ⚠️ not yet run against live agents, whose wording can change |
| **Notifications**: an agent turning *Needs you* ("claude needs you" + the question) or finishing a turn (*Working → Done*) while its terminal isn't on screen in the active window ("claude finished") posts one notification per transition, at most one per kind and terminal every 10 s: in-app toast (click → shows that terminal), system notification, Dock badge and sound, as set in Settings › Notifications. While a FerrumTerm window is connected it posts them (it knows what you're looking at); with no window open the server posts the system notifications itself | ⚠️ transition / debounce logic unit-tested; toasts not looked at on screen |
| **Folders** in the sidebar's terminal list (FerrumTerm window): *New folder…* in the + menu, or *Move to folder › New folder…* on a terminal's right-click menu; drag a terminal onto a folder (or onto the list's free space to take it out), or use *Move to folder* / *Remove from folder*; click a folder to collapse / expand; right-click it for *New terminal in this folder* (starts in the folder's directory if set, else the current one), *Rename…*, *Set directory…*, *Delete folder* (its terminals go back to the top level and keep running). Folders hold tabs, or sessions while tabs run across the top. Kept per machine in `<state dir>/sidebar-folders.json`; memberships are dropped (folders kept) when the server restarts, since it reuses ids | ⚠️ folder model unit-tested; drag-and-drop and menus not looked at on screen |
| Shortcuts: ⌘T tab · ⌘N session · ⌘D / ⌘⇧D split · ⌘W close · ⌘[ ⌘] pane · ⌘⇧[ ⌘⇧] / ⌘1–9 tabs · ⌘⇧S next session · ⌘B sidebar · ⌘C copy selection · ⌘V · ⌘⇧C copy screen · ⌘= ⌘- ⌘0 | ⚠️ wired, not hand-tested |

Protocol version 2 (forward requests): a server started by an older build refuses new clients
with a "protocol mismatch" message; run `fterm server restart` (or `fterm server stop`, and it
restarts on demand). Compatible but older servers are flagged by revision (see above).

## Not built yet
Selection by word/line (double/triple click), selecting into scrollback, mouse reporting to programs, OSC 8 hyperlinks, ligatures,
dead keys, kitty keyboard and graphics protocols, importing iTerm2 / Ghostty theme files, ⌘F search,
restoring sessions after the *server* restarts (programs can't survive their PTY owner),
built-in Tailscale node (Rex embeds Go's tsnet; there is no Rust equivalent, so FerrumTerm
listens on the tailnet IP of your running Tailscale), iOS client, Linux packaging (the code is
portable; only macOS was exercised), app bundle and signing.
SSH extras not built yet: a command-palette entry / shortcut for the ports dialog (it is a
sidebar action only), password / keyboard-interactive auth for background forwards (they use
BatchMode, so the key must be in ssh-agent), auto-restart of dropped forwards, restoring
forwards after the server restarts, seeing other users' listeners without `sudo` on macOS
(the dialog says when a port is held by a process it can't see).

## Commands
```bash
cargo term                         # GPUI window (starts the server if needed)
cargo fterm -- ls                  # CLI via cargo; or use target/debug/fterm directly
cargo test --workspace             # real PTYs, temp dirs, no network
cargo test -p ferrumterm-ui        # incl. a headless embedded TermView on a private server
cargo lint && cargo fmt --all
cargo build --release              # target/release/{ferrumterm,fterm}: put both on PATH
```

### Agents
```bash
fterm skill setup --hooks          # skill for Claude Code + Codex, plus Claude status hooks
fterm run --split right -l server -- npm run dev
fterm run --window -l agent -- claude          # agent in its own tab; close the window, it keeps going
fterm wait -t agent --status done,blocked,error
fterm capture -t agent -n 30
```

### SSH hosts, forwards and ports
```bash
fterm ssh --list                                   # saved + ~/.ssh/config hosts
fterm ssh add db me@db.internal -J bastion -i ~/.ssh/id_ed25519
fterm ssh db                                       # new tab running ssh
fterm forward add db L 5432:localhost:5432
fterm forward start db                             # all of db's forwards (server-owned ssh -N)
fterm forward list && fterm forward stop db/L5432
fterm port 3000 --kill                             # who listens on 3000; asks before SIGTERM/SIGKILL
fterm ports --json
```

### Remote (another Mac on your tailnet)
```bash
# on the machine running agents
fterm server start --tailscale     # or: fterm server install --tailscale
fterm server token                 # copy this
# on your laptop
fterm hosts add studio tcp://<tailnet-ip>:7397 --token <token>
fterm -S studio ls && fterm -S studio attach
```
Anyone holding the token can run programs as you. Treat it like an SSH key. Listeners refuse
any address that isn't loopback or in Tailscale's ranges.

## Layout
```
apps/gui/            FerrumTerm window: a thin binary showing `ferrumterm_ui::TermView` (standalone options)
apps/cli/            fterm: commands, server management + LaunchAgent, attach (crossterm), skill, hosts,
                     ssh, forward, port(s)
crates/protocol/     wire types, length-prefixed JSON framing, layouts, OSC 7501 parser/scanner,
                     port-forward specs (`L 8080:host:80` parsing)
crates/emulator/     alacritty_terminal wrapper: frames, capture, replies, key-spec encoding, palette
crates/server/       daemon: state/targets, PTY blocks, connections, listeners, keep-awake, notify,
                     port forwards (ssh -N children)
crates/client/       async client, endpoints, default paths, auto-start
crates/net/          ~/.ssh/config parser, saved SSH hosts + forwards store, ssh argv building,
                     lsof / ss / ps parsers and port lookup, process signalling
crates/ui/           ferrumterm-ui, the embeddable terminal UI: `TermView` (view/: state + events, render,
                     public api, IME), paint (frames as quads + shaped text), actions (key bindings in the
                     `FerrumTerm` key context), input, app (server state and requests), connection (shared
                     server or private in-process one), sidebar (model, agents, hosts), dialogs (host form,
                     ports), ssh, mouse selection, project bar, persisted window state (ui_state)
docs/                ARCHITECTURE
```

## Embedding the terminal (FerrumIDE, FerrumEngineer)
`ferrumterm-ui` is the whole FerrumTerm UI as one GPUI view; the desktop app is a thin binary
around it. A host adds `ferrumterm-ui.workspace = true` and:

```rust
use ferrumterm_ui::{TermView, TermViewEvent, TermViewOptions};

ferrumterm_ui::init(cx);                       // once: key bindings, in the `FerrumTerm` key context
let term = TermView::new(
    TermViewOptions::embedded()                // compact, sidebar collapsed
        .cwd(&project_root)                    // first session, new tabs/splits/commands
        .session_name("editor:my-app"),        // its own server session, created when missing
    window,
    cx,
);
cx.subscribe(&term, |this, _, event, cx| match event {
    TermViewEvent::AgentStatus { title, status, .. } => { /* working / blocked / done (OSC 7501) */ }
    TermViewEvent::CommandFinished { command: Some(id), exit_code, .. } => { /* run_command ended */ }
    TermViewEvent::TitleChanged(title) => { /* pane header */ }
    _ => {}
}).detach();
// render it as a child element: `div().size_full().child(term.clone())`

term.update(cx, |term, cx| {
    let id = term.run_command("cargo test", None, cx); // own tab, stays open, exit code in the event
    term.split(Axis::Horizontal, cx);                  // new pane to the right (Vertical: below)
    term.new_tab(Some(path), cx);
    term.set_cwd(new_root, cx);                        // for later tabs; running shells stay put
});
term.read(cx).focus_active(window, cx);
```

| `TermViewOptions` field | Default | Meaning |
|---|---|---|
| `cwd` | `None` | Directory for the first session and for tabs / splits / commands opened without one |
| `compact` | `false` (`embedded()`: `true`) | 12 pt font, 28 px tab bar, scrollable tabs, sidebar drawn *over* the terminal (closes when you pick something). Usable from 320 px wide |
| `show_sidebar` | `true` (`embedded()`: `false`) | Sidebar (agents, sessions, SSH hosts, ports) open at start; ⌘B or the tab-bar button toggles it |
| `attach_to_server` | `true` | Use the shared `fterm` server, starting it (`fterm server start`, with the `fterm` next to the host binary, else FerrumTerm.app's, else on `PATH`) if needed, so terminals survive the host closing and show up in FerrumTerm / `fterm attach`. If it can't be reached or started, or with `false`, a **private server runs inside the host process** (same PTYs and features; terminals end with the host) |
| `session_name` | `None` | Show this server session (created when missing) instead of the first one, so the host's terminals stay apart from FerrumTerm's own. Its events are filtered to that session |
| `standalone` | `false` | The desktop window only: sets the window title, shows the project bar, takes focus, saves the sidebar's state (embedded views only save recent agent commands), and keeps retrying the shared server instead of falling back to a private one (as before) |

Events (`TermViewEvent`): `Connected(Backend::Server \| Local)`, `TitleChanged(String)`,
`AgentStatus { block, title, status }` (from OSC 7501 / `fterm status` / Claude Code hooks; `status: None` = cleared),
`CommandStarted { command, block }`, `CommandFinished { block, command: Option<CommandId>, exit_code }`
(any pane's process exiting; `command` set for `run_command` tabs), `Bell { block }`.

Keys: `init` binds ⌘T ⌘N ⌘D ⌘⇧D ⌘W ⌘[ ⌘] ⌘⇧[ ⌘⇧] ⌘1–9 ⌘C ⌘V ⌘⇧C ⌘= ⌘- ⌘0 ⌘⇧S ⌘B ⌘PgUp/PgDn as actions
(`ferrum_term::NewTab`, `ClosePane`, …) in the `FerrumTerm` key context: while the terminal has focus they
win over the host's global bindings, elsewhere they do nothing. Hosts with their own keymap can register
`ferrumterm_ui::key_bindings()` (or a subset) instead. Calls made before the view is connected
(`run_command` right after `new`) are queued until its session exists.

## Local data
`~/Library/Application Support/dev.FerrumTerm.FerrumTerm/` holds `token` (0600), `server.log`
`hosts.json` (0600, remote FerrumTerm servers) and `ssh-hosts.json` (0600, saved SSH hosts and
port-forward definitions; no secrets); the window keeps sidebar state and recent custom agent commands
in `gui-state` (plain `key=value`). The socket is at `$TMPDIR/ferrumterm-$USER/server.sock`, in a 0700
directory with 0600 permissions. Override with `FTERM_STATE_DIR` / `FTERM_SOCKET`.
