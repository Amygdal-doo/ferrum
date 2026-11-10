# FerrumKit

Offline developer utilities in the style of DevUtils / DevToys, written in Rust: a native
GPUI desktop app (the UI framework of Zed, via the FerrumIDE fork) built on the shared
`ferrum-ui` crate. One window: a searchable list of tools on the left, the selected tool on
the right. Everything runs locally — FerrumKit makes no network requests and writes nothing
to disk. Part of the Ferrum monorepo, sibling of [FerrumAPI](../api), [FerrumGIT](../git),
[FerrumData](../data) and [FerrumIDE](../ide), with the same theme and components.

> **Status: alpha — first vertical slice.** See "What works" below; nothing beyond that list
> should be assumed. "Tested" means automated unit tests of `ferrumkit-core`; the GPUI UI
> builds and lints clean but has **not been launched or clicked through** yet.

## What works today
| Tool | State |
|------|-------|
| Member of the Ferrum monorepo workspace, pinned toolchain, workspace lints | ✅ |
| Shell: tool sidebar with search (name + keywords, all terms must match), ⌘K / Ctrl+K focuses search, Enter opens the first match; each tool keeps its last input in memory for the session; copy buttons with a "Copied" toast | ⚠️ filter unit-tested; UI wired (not hand-tested) |
| **JSON / YAML / TOML**: format (2/4 spaces, tab), minify, validate with error line + column; JSON ⇄ YAML ⇄ TOML (key order kept; TOML errors for non-table roots and `null`); "use output as input" | ✅ tested |
| **Base64**: encode / decode text, standard (padded) or URL-safe (unpadded); decode ignores whitespace and missing padding; non-UTF-8 results reported as binary | ✅ tested |
| **JWT decoder**: header and payload pretty-printed, `alg`, `exp`/`iat`/`nbf` as UTC + local dates + relative time, Expired / Not yet valid / Not expired / No exp badge; accepts a `Bearer ` prefix; detects JWE. **Decode only — signatures are not verified** (stated in the UI) | ✅ tested |
| **URL**: encode (like `encodeURIComponent`), decode (optional `+` as space), parser showing scheme / user / password / host / port / path / query / fragment and decoded query parameters (repeats kept) as rows | ✅ tested |
| **Unix timestamp**: seconds or milliseconds (auto-detect from 12 digits up), dates (RFC 3339, RFC 2822, `YYYY-MM-DD[ HH:MM[:SS]]`) read as local or UTC; shows seconds, ms, UTC, local, ISO 8601, relative; **Now** button | ✅ tested |
| **UUID / ULID**: UUID v4, UUID v7 (time-ordered), ULID; count 1–10 000; uppercase toggle for UUIDs | ✅ tested |
| **Hash**: MD5, SHA-1, SHA-256, SHA-512 of the UTF-8 text, live; uppercase toggle | ✅ tested against known vectors |
| **Regex tester** (Rust `regex` crate syntax — no look-around/backreferences): pattern + flags `i m s x`, matches highlighted in the text, match list with byte ranges and numbered / named capture groups; 1 000-match cap | ✅ tested |
| **Text diff**: line-based (via `similar`), old/new line numbers, added/removed colouring, summary counts, copy as `+`/`-` text; renders the first 5 000 lines | ✅ tested |
| **Number base**: decimal / hex / octal / binary, `0x`/`0o`/`0b` prefixes, `_`/`,`/space separators, negatives, up to 128 bits | ✅ tested |
| **Colour**: `#rgb[a]`, `#rrggbb[aa]`, `rgb()/rgba()`, `hsl()/hsla()` (comma or space syntax) → hex, RGB, HSL, channels, swatch | ✅ tested (HSL round-trips) |
| **Case converter**: camelCase, PascalCase, snake_case, SCREAMING_SNAKE, kebab-case, Title Case, lower, UPPER; line by line; splits acronyms (`HTTPServer` → `HTTP Server`) | ✅ tested |
| **Line tools**: sort A→Z / Z→A / ignoring case, reverse, dedupe (keeps first), trim, remove empty, shuffle; in place with undo; live counts (lines, non-empty, unique, words, chars, bytes) | ✅ tested |

## Not built yet
Persistence of inputs between launches; file input / drag-and-drop (all tools take pasted
text); Base64 of binary files and images; JWT signature verification and encoding; hashing
files, HMAC, other algorithms (SHA-3, BLAKE3, CRC32); regex replace and other regex engines
(PCRE/JavaScript syntax); character-level (intra-line) diff and side-by-side diff view; JSON
path queries, JSON schema, XML / CSV conversion; colour picker, contrast ratio and named
colours; keyboard navigation of the tool list (arrow keys) and ⌘1… shortcuts; soft-wrap in
text fields (long lines such as JWTs scroll horizontally — the shared text field lays out
unwrapped lines); a light theme; packaging, signing and a Ferrum Launcher entry.

## Prerequisites
- Rust via rustup. `rust-toolchain.toml` installs the pinned toolchain automatically.
- macOS: Xcode command line tools (GPUI renders with Metal). Linux/Windows: whatever GPUI
  needs on that platform (see `ide/` docs).
- No Node, no CSS build: styling is Rust code using the FerrumIDE theme.

## Commands
Run from the monorepo root (`ferrum/`).
```bash
cargo run -p ferrumkit-desktop                 # run the app (binary: ferrumkit)
cargo test -p ferrumkit-core -p ferrumkit-desktop
cargo clippy -p ferrumkit-core -p ferrumkit-desktop --all-targets -- -D warnings
cargo fmt --all
```

## Layout
```
apps/desktop/            GPUI app: app.rs (root view: search, sidebar, tool host, ⌘K),
                         widgets.rs (fields, copy button, choice buttons, value rows),
                         tools/ (one view per tool)
crates/core/             pure, unit-tested tool logic, no GPUI: catalog (tool list + search),
                         data_format (JSON/YAML/TOML), base64, jwt, url_tool, timestamp, ids,
                         hash, regex_tool, diff, number_base, color, case, lines
```

## Local data
None. Inputs live in memory while the app runs and are gone when it quits.
