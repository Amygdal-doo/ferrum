# Ferrum

A local-first, DataGrip-class database IDE written in Rust: Dioxus 0.7 desktop, Rust/UI
components, SQLx. No backend server, no React/TypeScript/Tauri/Electron.

> **Status: M0/M1 foundation + first PostgreSQL slice.** See "What works" below; nothing
> beyond that list should be assumed. The full specification lives in your planning
> docs; decisions and open risks are in [`docs/ADR`](docs/ADR/README.md).

## What works today
| Area | State |
|------|-------|
| Cargo workspace, pinned toolchain (1.99.1), CI (fmt, clippy, tests on macOS/Linux/Windows, PG integration) | ✅ |
| Desktop shell: header, connections sidebar, tabbed editor, results panel, status bar, resizable + persisted panels, light/dark/system theme | ✅ (manual smoke tested for start/quit only, see QA plan) |
| PostgreSQL profiles: create/edit/test/delete, TLS mode, environment tag, read-only session | ✅ |
| Passwords in OS keychain (or session-only), never in the app DB | ✅ |
| Query execution: run selection or statement under caret (⌘/Ctrl+Enter), streaming batches, bounded channels, row limit, timeout, cancel with server-side `pg_cancel_backend` | ✅ integration-tested |
| Virtualized results grid with typed rendering (NULL, numbers, bool, binary, JSON, timestamps) | ✅ (benchmark numbers pending, ADR-004) |
| Production confirmation for mutating statements | ✅ (UX guard, not security) |
| Table explorer: expand a connection → tables/views by schema (lazy, filterable, capped render) | ✅ integration-tested introspection |
| Table browser tab: breadcrumb, refresh, editable SQL bar, ordered by primary key (warns if none), auto-sized columns | ✅ |
| Editor tabs + SQL text (and table tabs) restored after restart | ✅ |

## Not built yet (from the spec)
Row editing / "+ Row" / column add (IDE-014), column and index details in the tree, syntax highlighting/autocomplete (gated on ADR-003 spike),
query history (IDE-013), CSV/JSON export (IDE-017), transactions UI, MySQL/SQLite adapters (IDE-015/016), column resize/sort/copy, SSH, packaging
and signing, MongoDB/Redis, AI. The grid shows at most `max_rows` (default 10,000).

## Prerequisites
- Rust via rustup. `rust-toolchain.toml` installs **1.99.1** automatically. SQLx 0.9
  needs ≥ 1.94.
- macOS: Xcode command line tools. Linux: `libwebkit2gtk-4.1-dev libxdo-dev libssl-dev
  libayatana-appindicator3-dev librsvg2-dev`. Windows: WebView2 (preinstalled on 10/11).
- Optional: Docker (local PostgreSQL), Node 22 + pnpm (only to rebuild CSS),
  `cargo install dioxus-cli@0.7.10 --locked` (only for `dx serve` / `dx bundle`).

## Commands
```bash
cargo desktop                    # run the app (alias for: cargo run -p ferrum-desktop)
cargo test --workspace           # unit + service tests
cargo lint                       # clippy, warnings are errors
cargo fmt --all

docker compose up -d postgres    # local PG on 127.0.0.1:54329 (ferrum / ferrum-dev-password, db ferrum_test)
FERRUM_TEST_PG=1 cargo test -p ferrum-db-postgres --test postgres

cargo test -p ferrum-credentials -- --ignored   # real OS keychain round trip (may prompt)
RUST_LOG=debug cargo desktop                     # verbose logs (never include SQL or secrets)
```

### Styling
Tailwind v4 compiles `apps/desktop/tailwind.css` (scanning `src/**/*.rs`) into
`apps/desktop/assets/tailwind.css`. That file is committed and embedded at compile time,
so `cargo run` needs no Node. After changing Tailwind classes:
```bash
pnpm --dir apps/desktop install
pnpm --dir apps/desktop css      # or css:watch
```
CI fails if the committed CSS is stale.

### Packaging
`apps/desktop/Dioxus.toml` is in place for `dx bundle`, but bundling, signing and
notarization are **not yet tested** (ADR-009, IDE-018).

## Layout
```
apps/desktop/            Dioxus app: components/{shell,connection,sql_editor,result_grid,dialogs,rust_ui}, state/, commands/
crates/domain/           pure types: profiles, Secret, CellValue, QueryEvent, QueryState
crates/db-driver-api/    DatabaseDriver / DbConnection traits, bounded event channel, cancellation
crates/db-postgres/      SQLx PostgreSQL adapter (streaming, cancel, exact NUMERIC decoding)
crates/application/      AppServices runtime, ConnectionService, QueryService, safety heuristics
crates/persistence/      SQLite app state + migrations (settings, profiles, tabs)
crates/credentials/      OS keychain (keyring 4) + session-memory fallback
crates/editor-core/      statement splitter, cursor → statement, UTF-16 ↔ byte offsets
crates/result-grid-core/ viewport math, bounded ResultBuffer
docs/                    ARCHITECTURE, SECURITY, DRIVER_CONTRACT, QA_TEST_PLAN, ADR/
```

## Local data
`~/Library/Application Support/dev.Ferrum.Ferrum/ferrum.db` on macOS (OS-appropriate
elsewhere via `directories`). Deleting it resets Ferrum. Saved passwords live in the OS
keychain under the service `dev.ferrum.connection`.
