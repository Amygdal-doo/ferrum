# FerrumAPI

A local-first, Postman-style API client written in Rust: a native GPUI desktop app (the UI
framework of Zed, via the FerrumIDE fork) built on the shared `ferrum-ui` crate,
reqwest + rustls, SQLite. No backend server, no account, no Electron, no webview. Part of
the Ferrum monorepo — sibling of [FerrumData](../data), [FerrumGIT](../git) and
[FerrumIDE](../ide), with the same theme and components.

> **Status: M0 — first vertical slice.** See "What works" below; nothing beyond that list
> should be assumed. "Tested" means automated tests; the GPUI UI has only been smoke-launched
> (start → 8 s → quit, no errors, empty and existing local data), not clicked through by a
> human yet.

## What works today
| Area | State |
|------|-------|
| Member of the Ferrum monorepo workspace, pinned toolchain (1.99.1), workspace lints, `cargo api` / `cargo lint` aliases | ✅ |
| Request execution: GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS over reqwest + rustls on a dedicated Tokio runtime | ✅ integration-tested against a local mock server |
| Bodies: none, raw (JSON/Text/XML/HTML, default Content-Type), x-www-form-urlencoded, multipart form-data (text + file), binary file | ✅ integration-tested |
| Auth: none, Basic, Bearer, API Key (header or query), inherited from folder/collection (tab's unsaved auth respected) | ✅ tested (http + application) |
| Send / cancel (CancellationToken), timeout, follow redirects, verify TLS | ✅ integration-tested (cancel returns in ~1 s; redirect on/off; loop → error) |
| `{{variable}}` substitution in URL, params, headers, body and auth; globals < collection < environment layering; unresolved detection | ✅ tested |
| Response data: status, time, size, headers, cookies (Set-Cookie parsed), full body bytes | ✅ tested |
| Collections with nested folders: create, rename, duplicate, delete, save requests into them | ✅ domain tree ops + SQLite persistence tested; UI wired (not hand-tested) |
| Environments + globals CRUD, active-environment selector | ✅ persistence tested; UI wired (not hand-tested) |
| History of sent requests (authored request + status/time/size; pruned to 500), reopen in a tab | ✅ recorded end-to-end in tests; UI wired (not hand-tested) |
| Multiple request tabs, restored after restart (debounced autosave) | ✅ reducers unit-tested; autosave observed writing to SQLite on smoke launch |
| Postman Collection v2.1 import/export (v2.0 import), Postman environment import | ✅ fixture + round-trip tested; file import/export end-to-end tested |
| cURL import (incl. Chrome "Copy as cURL", `$'…'`, `-sSL`, `-F`, `-u`, `-G`) | ✅ tested |
| Code generation: cURL, Python `requests`, JavaScript `fetch` | ✅ snapshot + cURL round-trip tested |
| Response body search (⌘F / Ctrl+F): highlight all matches, current-match highlight, count, next/previous (Enter, ⌘G / ⇧⌘G, buttons), match-case toggle, Esc closes; scrolls to the match. Works in Pretty and Raw | ✅ matching unit-tested (case folding, Unicode offsets, non-overlap, 10 000-match cap); UI wired (not hand-tested) |
| JSON syntax colouring in Pretty view (keys, strings, numbers, booleans, null) from the theme's syntax colours; the pretty text is cached per response instead of re-parsed every frame | ✅ tokenizer + span layering unit-tested; UI wired (not hand-tested) |
| Cookie jar: `Set-Cookie` cookies stored for the workspace (SQLite, migration `0002_cookies`), sent on later requests to matching domain/path (host-only vs `Domain`, default path, `Secure` only over https, `Expires`/`Max-Age` expiry, deletion via past expiry); an explicit `Cookie` header wins per name. **Cookies** button in the header lists cookies by domain with delete / clear domain / clear all | ✅ matching + expiry unit-tested, persistence tested, end-to-end tested against a mock (login cookie → authenticated request → clear); dialog wired (not hand-tested) |
| Collection runner: **Run…** on a collection or folder (tree menu) sends its saved requests one at a time in tree order (inherited auth, current variables, cookie jar); per-request status, code, time, pass (2xx) / fail / error, Stop, Run again, click a row to open the request; closing the dialog keeps the run going (status bar shows progress and reopens it). Runs are not added to history | ✅ sequencing + stop unit-tested, run plan tested, end-to-end tested against a mock; dialog wired (not hand-tested) |
| UI (GPUI): request builder with two-way URL ↔ params sync, header/param enable toggles, body/auth/settings editors, unresolved-variable highlighting, response Pretty/Raw/Preview (images inline; HTML opens in the default browser), headers/cookies tables, copy + save to file, sidebar (Collections/Environments/History) with context menus, import + code dialogs, resizable panes | ⚠️ builds, lints clean, launches; not yet clicked through |

## Not built yet
OAuth 1/2, Digest, AWS and other auth types (imported as "Inherit" with a note); pre-request /
test scripts (the runner's pass/fail is 2xx only); runner options (iterations, delay, data
files, run report export); WebSocket, GraphQL editor (GraphQL bodies import as raw JSON), gRPC;
cookie jar gaps: cookies set on intermediate redirect responses are not captured (only the
final response's), cookies are chosen for the request URL before redirects, no public-suffix
list, session cookies persist until cleared, no per-request "don't use the jar" switch, one jar
per workspace (not per collection), and cookies are not editable in the dialog; syntax
colouring for XML/HTML bodies; regex / whole-word search; proxy and client-certificate
settings; drag-and-drop reordering / moving items between folders; keeping text per body mode when switching (switching starts the new mode empty); response
bodies over 400 KB are truncated for display (copy/save use the full body); secrets stored in
the OS keychain (they live in the local SQLite file, never in logs); packaging and signing;
a light theme (the GPUI shell ships FerrumIDE's dark "One Dark" base theme only; the stored
`theme` setting is kept but not applied); soft-wrap in the response body view (the shared
text field lays out unwrapped lines; wrapping needs a new layout path there).

## Prerequisites
- Rust via rustup. `rust-toolchain.toml` installs **1.99.1** automatically.
- macOS: Xcode command line tools (GPUI renders with Metal). Linux/Windows: whatever GPUI
  needs on that platform (see `ide/` docs); only macOS has been smoke-tested.
- No Node, no CSS build: styling is Rust code using the FerrumIDE theme.

## Commands
Run from the monorepo root (`ferrum/`).
```bash
cargo api                        # run the app (alias for: cargo run -p ferrumapi-desktop)
cargo test -p 'ferrumapi-*'      # unit + integration tests (local mock servers, no internet)
cargo lint                       # clippy, warnings are errors
cargo fmt --all

RUST_LOG=debug cargo api         # verbose logs (never include URLs, headers, bodies or secrets)
```

### Styling
There is no stylesheet. Components come from Zed's `ui` crate and `ferrum-ui`
(`../shared/ui`: app bootstrap, theme, text input, splitter, notices, modal); colours come
from the active theme (`cx.theme()`).

### Packaging
Bundling, signing and notarization are **not yet set up** for the GPUI build.

## Layout
```
apps/desktop/            GPUI app: app.rs (root view), components/{shell,request,response,sidebar,dialogs,widgets,fields}, state/, commands/
crates/domain/           pure serde types: HttpRequest, RequestBody, Auth, HttpResponse, Collection tree ops,
                         Environment, VariableSet ({{var}} scan/resolve), URL ↔ params sync, history, workspace,
                         CookieJar (RFC 6265 storage/matching), highlight (find matches, JSON tokens, span layering)
crates/http/             reqwest + rustls execution: substitution, auth, bodies, cookies, cancellation, error mapping
crates/persistence/      SQLite app state + migrations (collections, environments, globals, history, tabs, settings, cookies)
crates/interop/          Postman v2.1 import/export, Postman environments, cURL import, code generation
crates/application/      AppServices runtime, prepare (auth inheritance + variable layers), send + history, cookie jar,
                         collection runner, file transfer
docs/                    ARCHITECTURE
```

## Local data
`~/Library/Application Support/dev.FerrumAPI.FerrumAPI/ferrumapi.db` on macOS (OS-appropriate
elsewhere via `directories`). Deleting it resets FerrumAPI. Response bodies are never stored;
cookies from the cookie jar are (clear them from the **Cookies** dialog).
