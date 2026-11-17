# Architecture

```
apps/gui (GPUI window) ───────┐
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
- Command blocks: OSC 133 marks (prompt `A`, input `B`, output `C;cmdline_url=…`, done `D;<code>`)
  are scanned the same way, and the feed is split at each so it applies at its place in the
  stream: it sets the cursor template's hyperlink to a hidden id (`fterm-cmd:<n>:<p|i|o>`), so
  every cell written after it carries its command and zone. Marks thus live in the grid (they
  scroll, reflow and leave with the scrollback); a row belongs to its first marked cell's command,
  blank rows to the row above. Frames carry the commands on screen (`Frame::commands`);
  `Commands` / `CommandOutput` scan the whole grid on request. The shell scripts are written to
  `<state dir>/shell-integration/` and loaded into zsh (a `ZDOTDIR` bootstrap) and fish
  (`XDG_DATA_DIRS`) when a block starts one.
- Find: `Find` searches the screen and scrollback with a regex (rows joined by a soft wrap are
  one line); matches are in lines from the oldest scrollback line, stable while output scrolls.
- Security: the Unix socket is trusted (0600 in a 0700 dir). TCP requires `Hello` with the token
  (constant-time compare) as the first message and binds only to loopback or Tailscale ranges.

## Protocol
`u32` big-endian length + JSON. Clients send `{id, op, …}`. The server replies with `{reply: {id, res}}`
or pushes `{event: …}`. `Wait` replies asynchronously with the same id, so one connection
can wait and keep working. Targets are strings resolved server-side (`b3`, `w2`, `s1`, a session
name, a window name, or a block label).

## GUI
The window is a GPUI view built on the shared `ferrum-ui` crate (theme, fonts and window
bootstrap shared with the other Ferrum tools). A canvas paints each pane from its
latest frame: background quads per span, then each row shaped with per-span `TextRun`s
(bold/italic/dim/underline/strike) in the monospace buffer font. Narrow characters are shaped
together with glyph advances forced to the cell width; wide characters (CJK, emoji) are shaped on
their own so they cover two cells. The cursor, dividers and the unfocused-pane hollow cursor are
quads. The tab bar and message strip are ordinary GPUI elements. GPUI's line-layout cache means
unchanged rows are not reshaped. The view's tokio side (client connection, requests) talks to the
GPUI foreground through an unbounded channel. Printable text arrives through GPUI's input handler,
so IME composition works (the marked text is drawn at the cursor); everything else goes through
`input::translate`. Keys are sent as key specs, so the server encodes them against the
program's current modes (application cursor, bracketed paste).
