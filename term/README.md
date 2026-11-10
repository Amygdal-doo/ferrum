# FerrumTerm

A Rust terminal for humans and AI agents, inspired by Ghostty (GPU-rendered window), tmux
(persistent sessions, attach from anywhere) and Rex (agent status, scriptable CLI,
Tailscale access). Every line is Rust: `alacritty_terminal` for VT emulation,
`portable-pty`, Tokio, GPUI (via the shared `ferrum-ui` crate, same UI stack as FerrumIDE) for
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
Closing the window only disconnects a client. Programs keep running, and you can reopen
FerrumTerm, `fterm attach` over SSH, or reach the server from another Mac on your tailnet.

**Agents with the display off:** while any block reports `working`, the server holds
`caffeinate -i -s` (no idle sleep, and no system sleep on AC power), so agents keep
working with the screen off or locked. *A MacBook with its lid closed still sleeps*
unless it is in clamshell mode (power + external display). macOS doesn't let a user process
override that. Use `fterm awake always` to hold the assertion permanently.

## What works today
| Area | State |
|------|-------|
| Persistent server: sessions → windows (tabs) → split blocks, PTYs, 10k-line scrollback | ✅ tested |
| `fterm server start` (detached, survives the terminal closing), `stop`, `status`, `logs`, `install` (LaunchAgent: start at login, restart on crash) | ✅ start/stop/status tested; LaunchAgent written but not installed in testing |
| CLI like Rex's: `new ls run split window send send-key capture kill rename focus inspect wait status events awake`, `--json`, targets by id/name/label, `$FTERM_BLOCK` default | ✅ smoke-tested end to end |
| Program status: OSC 7501 (Rex-compatible) or `fterm status`; shown in `ls`, tabs and attach; `wait --status`; macOS notifications on done/blocked/error | ✅ tested |
| Keep-awake while agents work (`caffeinate`), `fterm awake auto/always/never` | ✅ implemented; manual check only |
| Agent skill for Claude Code (`~/.claude/skills/fterm`) and Codex (`~/.agents/skills/fterm`); `--hooks` adds Claude Code hooks for working/done/blocked status | ✅ implemented, not yet used with a live agent |
| tmux-style `fterm attach` (renders splits, status line, `C-b` prefix: d c % " o n p 0-9 x [ ]) | ✅ attach/detach checked in a real PTY |
| Remote: TCP listeners on loopback/Tailscale addresses only, 256-bit token, `--tailscale` auto-detects this Mac's tailnet IP, `fterm hosts add` + `fterm -S host …` | ✅ token auth tested on loopback; not tested across two machines |
| GPUI window: tabs, splits with focus accent, truecolor/256/16 colours, bold/italic/underline/strike/dim/inverse, block/beam/underline cursor, wide (CJK/emoji) cells, scrollback (wheel / Cmd+PgUp), paste (bracketed), copy screen, font size, IME composition | ⚠️ ported to GPUI; launch-tested, not yet visually re-checked |
| Sidebar (⌘B or the tab-bar button; open/closed and width remembered): **Agents** section (Claude Code, Codex, aider…, `agent…` labels, or anything reporting a status) with a coloured dot + `working` / `blocked · permission` / `done` / `error` and the status message; a badge counting agents that are blocked, or done / failed since you last looked at their tab; below it every session with its tabs and split panes (label, program, cwd). Click a row to show that session and focus the pane | ⚠️ builds; model unit-tested; not yet looked at on screen |
| **+ New agent** menu in the sidebar: Claude Code, Codex, recent custom commands, or "Custom command…" (inline field). Opens a new tab labelled after the program (`claude`, `claude-2`…), like `fterm run --window -l claude -- claude`, in the focused pane's directory, through your login shell so `.zprofile`/`.zshrc` PATH applies | ⚠️ builds; label/command logic unit-tested; not launched by hand |
| **SSH hosts** in the sidebar: hosts saved in FerrumTerm (name, user, host, port, identity file, jump host, startup command) plus the concrete `Host` aliases of `~/.ssh/config` (wildcards skipped, `Include` followed, never written). Click one → new tab running the system `ssh` (`-p` / `-i` / `-J`, `-t host '<cmd>'` for a startup command; config hosts run `ssh <alias>`). Add / edit / delete in a `Modal` form (pencil on the row); config hosts are read-only there. No passwords or keys are stored: ssh / ssh-agent handle passphrases. `fterm ssh <name>`, `fterm ssh --list [--json]`, `fterm ssh add <name> user@host [-p -i -J -c]`, `fterm ssh remove <name>` | ⚠️ parsing, validation and argument building unit-tested; CLI smoke-tested; GUI builds, not looked at on screen; no real ssh session opened in testing |
| **Port forwards** per host (`L [bind:]port:host:port`, `R …`, `D [bind:]port`), edited in the host form; each row starts / stops on click with a green (running) / red (failed, with ssh's error) / grey dot. They run as `ssh -N -o ExitOnForwardFailure=yes -o BatchMode=yes` children **owned by the server**, so they survive closing the window, and stop with the server. A taken local port is refused up front, naming the process holding it. `fterm forward list [--json] / add <host> <spec> / remove <host> <id> / start <host>[/L8080] [spec] / stop <key\|host\|--all>` | ⚠️ start/stop/failure/port-taken covered by a server test with a stand-in command (no real ssh); BatchMode means the key must be in ssh-agent / keychain |
| **What's using port N?** Sidebar "What's using a port?" dialog (all listening ports, or one), and `fterm port <N> [--kill] [--json]`, `fterm ports [--json]`: PID, process, user, address, command line. macOS: `lsof +c 0 -nP -iTCP:<N> -sTCP:LISTEN` + `ps`; Linux: `ss -ltnp`, falling back to lsof. Kill asks first (SIGTERM), then asks again before SIGKILL; never automatic | ✅ lsof / ss / ps parsers tested on fixtures; a test finds its own listener; kill path not exercised on real processes |
| Mouse selection (drag inside a pane) and ⌘C copy; the text is captured on mouse-up, cleared by typing, paste or scrolling | ⚠️ selection/hit-testing unit-tested; not hand-tested |
| Shortcuts: ⌘T tab · ⌘N session · ⌘D / ⌘⇧D split · ⌘W close · ⌘[ ⌘] pane · ⌘⇧[ ⌘⇧] / ⌘1–9 tabs · ⌘⇧S next session · ⌘B sidebar · ⌘C copy selection · ⌘V · ⌘⇧C copy screen · ⌘= ⌘- ⌘0 | ⚠️ wired, not hand-tested |

Protocol version 2 (forward requests): a server started by an older build refuses new clients
with a "protocol mismatch" message; run `fterm server stop` once and it restarts on demand.

## Not built yet
Selection by word/line (double/triple click), selecting into scrollback, mouse reporting to programs, OSC 8 hyperlinks, ligatures,
dead keys, kitty keyboard and graphics protocols, a config file and themes, ⌘F search,
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
apps/gui/            FerrumTerm window: GPUI view painting frames as quads + shaped text (view, input, app),
                     sidebar (model, agents, hosts), dialogs (host form, ports), ssh, mouse selection,
                     persisted window state (ui_state)
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
docs/                ARCHITECTURE
```

## Local data
`~/Library/Application Support/dev.FerrumTerm.FerrumTerm/` holds `token` (0600), `server.log`
`hosts.json` (0600, remote FerrumTerm servers) and `ssh-hosts.json` (0600, saved SSH hosts and
port-forward definitions; no secrets); the window keeps sidebar state and recent custom agent commands
in `gui-state` (plain `key=value`). The socket is at `$TMPDIR/ferrumterm-$USER/server.sock`, in a 0700
directory with 0600 permissions. Override with `FTERM_STATE_DIR` / `FTERM_SOCKET`.
