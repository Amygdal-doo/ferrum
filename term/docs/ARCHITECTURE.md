# Architecture

```
apps/gui (winit event loop) ──┐
apps/cli (fterm, attach) ─────┼── crates/client ── Unix socket / TCP+token ── crates/server
agents (fterm via skill) ─────┘                                                │
                                                     per block: PTY + reader thread + crates/emulator
crates/protocol ◄── all crates (wire types, layout math, OSC 7501)
```

## Server
- `State` holds sessions → windows (each a binary split `Layout`) → block ids, plus
  `last_focus`. A `std::sync::Mutex` guards it, with short critical sections. Never take two
  guards on one mutex in a single expression: that deadlocked `info()` once, and the tests caught it.
- `Block` owns the PTY master, writer, killer and a `Mutex<Terminal>`. A dedicated reader
  thread feeds output to the emulator, writes back terminal replies (DSR, colour queries),
  bumps a `watch` generation counter, and sends status, title and bell to the server loop.
  A wait thread reports the exit code.
- Exited blocks are removed (and layouts collapse) unless `keep_open`. Exit codes are remembered
  (last 512), so `wait` works even after a short-lived block is gone.
- Frames are pushed per subscription. Each subscribed block has a task that sends a full frame,
  then waits on the generation counter with an 8 ms coalescing delay (~120 fps cap). Frames are
  rows of styled spans with resolved RGB, so clients stay simple.
- Status: OSC 7501 is scanned from the raw byte stream (alacritty ignores unknown OSCs) or set
  with `SetStatus`. `working`/`blocked`/`idle` clear when the process exits; `done`/`error` stay.
  Transitions to done, blocked or error trigger a notification. Any `working` block triggers keep-awake.
- Security: the Unix socket is trusted (0600 in a 0700 dir). TCP requires `Hello` with the token
  (constant-time compare) as the first message and binds only to loopback or Tailscale ranges.

## Protocol
`u32` big-endian length + JSON. Clients send `{id, op, …}`. The server replies with `{reply: {id, res}}`
or pushes `{event: …}`. `Wait` replies asynchronously with the same id, so one connection
can wait and keep working. Targets are strings resolved server-side (`b3`, `w2`, `s1`, a session
name, a window name, or a block label).

## GUI
One wgpu render pass draws coloured quads (backgrounds, decorations, cursor, dividers, tab bar)
and then glyphon text. Each pane is a cosmic-text buffer with monospace width forced to the cell
width, rebuilt only when that pane's frame changes. Bitmap-only system faces (macOS
`GB18030Bitmap`) are removed from the font database: their broken metrics produced infinite
glyph widths for CJK fallback. Keys are sent as key specs, so the server encodes them against the
program's current modes (application cursor, bracketed paste).
