# FerrumTerm

A Rust terminal for humans and AI agents, inspired by Ghostty (GPU window), tmux
(persistent sessions, attach from anywhere) and Rex (agent status, scriptable CLI,
Tailscale access). Every line is Rust: `alacritty_terminal` for VT emulation,
`portable-pty`, Tokio, winit + wgpu + glyphon for the GPU window, crossterm for `attach`.

> **Status: M0, first vertical slice.** The server, CLI and attach are covered by
> automated tests (real PTYs, no network). The GPU window was launched and screenshotted on a demo
> session (splits, tabs, colours, CJK, status badges, close → reopen) but has not been
> used day-to-day by a person yet.

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
| GPU window: tabs, splits with focus accent, truecolor/256/16 colours, bold/italic/underline/strike/dim/inverse, block/beam/underline cursor, CJK and emoji fallback, scrollback (wheel / Cmd+PgUp), paste (bracketed), copy screen, font size | ✅ rendered and screenshotted |
| Shortcuts: ⌘T tab · ⌘N session · ⌘D / ⌘⇧D split · ⌘W close · ⌘[ ⌘] pane · ⌘⇧[ ⌘⇧] / ⌘1–9 tabs · ⌘⇧S next session · ⌘V · ⌘⇧C copy screen · ⌘= ⌘- ⌘0 | ⚠️ wired, not hand-tested |

## Not built yet
Mouse selection and copy, mouse reporting to programs, OSC 8 hyperlinks, ligatures,
IME/dead keys, kitty keyboard and graphics protocols, a config file and themes, ⌘F search,
restoring sessions after the *server* restarts (programs can't survive their PTY owner),
built-in Tailscale node (Rex embeds Go's tsnet; there is no Rust equivalent, so FerrumTerm
listens on the tailnet IP of your running Tailscale), iOS client, Linux packaging (the code is
portable; only macOS was exercised), app bundle and signing.

## Commands
```bash
cargo term                         # GPU window (starts the server if needed)
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
apps/gui/            FerrumTerm window: winit + wgpu quads + glyphon text (gpu, view, input, app)
apps/cli/            fterm: commands, server management + LaunchAgent, attach (crossterm), skill, hosts
crates/protocol/     wire types, length-prefixed JSON framing, layouts, OSC 7501 parser/scanner
crates/emulator/     alacritty_terminal wrapper: frames, capture, replies, key-spec encoding, palette
crates/server/       daemon: state/targets, PTY blocks, connections, listeners, keep-awake, notify
crates/client/       async client, endpoints, default paths, auto-start
docs/                ARCHITECTURE
```

## Local data
`~/Library/Application Support/dev.FerrumTerm.FerrumTerm/` holds `token` (0600), `server.log`
and `hosts.json` (0600). The socket is at `$TMPDIR/ferrumterm-$USER/server.sock`, in a 0700
directory with 0600 permissions. Override with `FTERM_STATE_DIR` / `FTERM_SOCKET`.
