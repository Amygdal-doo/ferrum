# FerrumData

A local-first, DataGrip-class database IDE written in Rust: GPUI desktop (the UI framework
from Zed, via the FerrumIDE fork) with the shared `ferrum-ui` crate, SQLx. No backend server,
no webview, no React/TypeScript/Tauri/Electron. Part of the Ferrum monorepo (`../`).

> **Status: M0/M1 foundation + PostgreSQL, MySQL, SQLite, SurrealDB and MongoDB drivers.**
> See "What works" below; nothing
> beyond that list should be assumed. The full specification lives in your planning
> docs; decisions and open risks are in [`docs/ADR`](docs/ADR/README.md).

## What works today
| Area | State |
|------|-------|
| Cargo workspace, pinned toolchain (1.99.1), CI (fmt, clippy, tests on macOS/Linux/Windows, PG integration) | ✅ |
| Desktop shell: header, connections sidebar, tabbed editor, results panel, status bar, resizable + persisted panels | ✅ (manual smoke tested for start/quit only, see QA plan) |
| Theme setting (system/light/dark) | ⚠️ persisted, but only the built-in One Dark theme is rendered since the GPUI port (ADR-011) |
| PostgreSQL profiles: create/edit/test/delete, TLS mode, environment tag, read-only session | ✅ |
| Engine picker in the connection form; fields per engine (host/port, SQLite file + native file picker, MongoDB connection string, SurrealDB namespace) | ✅ (compiles; not yet manually smoke tested in the running app) |
| Saved profiles from before multi-engine support keep loading | ✅ unit-tested (domain + persistence) |
| Passwords in OS keychain (or session-only), never in the app DB | ✅ |
| Query execution: run selection or statement under caret (⌘/Ctrl+Enter), streaming batches, bounded channels, row limit, timeout, cancel with server-side `pg_cancel_backend` | ✅ integration-tested |
| Virtualized results grid with typed rendering (NULL, numbers, bool, binary, JSON, timestamps) | ✅ (benchmark numbers pending, ADR-004) |
| Production confirmation for mutating statements | ✅ (UX guard, not security) |
| Table explorer: expand a connection → tables/views by schema (lazy, filterable, capped render) | ✅ integration-tested introspection |
| Table browser tab: breadcrumb, refresh, editable SQL bar, ordered by primary key (warns if none), auto-sized columns | ✅ |
| Browse queries per engine (backtick/double-quote quoting, SurrealQL `SELECT … ORDER BY id`, MongoDB `db.getCollection(…).find({}).sort({_id: 1})`) | ✅ unit-tested; SurrealDB/MongoDB forms also run live |
| Editor tabs + SQL text (and table tabs) restored after restart | ✅ |

### Drivers
"Live" means the driver contract tests (`crates/db-*/tests/`) passed against a real server
(Docker images from `docker-compose.yml`) during development; they are skipped in a plain
`cargo test` unless the env var is set. Not yet wired into CI.

| Engine | Crate | Editor input | Connect/auth errors | Types / NULL vs '' | Batching + `max_rows` | Cancel | Timeout | Read-only | Table list |
|--------|-------|--------------|---------------------|--------------------|-----------------------|--------|---------|-----------|------------|
| PostgreSQL | `db-postgres` | SQL | ✅ live (CI) | ✅ live | ✅ live | ✅ server-side `pg_cancel_backend` | ✅ live | ✅ session default | ✅ live |
| MySQL 8 / MariaDB | `db-mysql` | SQL (text-protocol fallback for statements MySQL cannot prepare) | ✅ live (MySQL 8.4) | ✅ live (incl. unsigned, DECIMAL, negative TIME) | ✅ live | ✅ server-side `KILL QUERY` | ✅ live | ✅ `SET SESSION TRANSACTION READ ONLY` | ✅ live (scoped to the profile's database if set) |
| SQLite | `db-sqlite` | SQL; profile = existing file path | ✅ (missing file) | ✅ storage-class decoding, `BOOLEAN` columns as bool | ✅ | ✅ engine-side interrupt (progress handler) | ✅ | ✅ opened read-only by SQLite | ✅ |
| SurrealDB 3.x | `db-surrealdb` | SurrealQL over `ws://` / `wss://` (no embedded engine) | ✅ live (root, then database, then namespace user) | ✅ live; record ids as `table:id`, nested values as JSON | ✅ live | ⚠️ client-side only: the result stream is dropped, the server may finish the statement | ✅ live (client-side) | ⚠️ FerrumData refuses writing keywords (UX guard) | ✅ live (`INFO FOR DB`, views = `AS SELECT` tables) |
| MongoDB 8 | `db-mongodb` | mongo-shell subset or JSON command (below) | ✅ live (no-auth server; bad credentials, unreachable host) | ✅ live; ObjectId as hex, dates with offset, Decimal128 exact, nested values as Extended JSON | ✅ live | ✅ cursor killed on drop; a running operation is bounded by `maxTimeMS` | ✅ live (`maxTimeMS` + client) | ⚠️ FerrumData refuses write methods/commands (UX guard) | ✅ live (collections + views) |

SQLite tests need no server and always run. Document engines (SurrealDB, MongoDB) buffer up
to `max_rows` documents before sending column metadata, so the grid's columns are the union
of every fetched document's top-level fields; a field a document lacks shows as `<absent>`
(distinct from `NULL`).

**MongoDB query input.** One call per run, written like the mongo shell:
`db.users.find({ age: { $gt: 30 } }, { name: 1 }).sort({ name: 1 }).skip(10).limit(20)`,
`db.users.findOne({…})`, `db.orders.aggregate([…])`, `db.users.countDocuments({…})`,
`db.users.estimatedDocumentCount()`, `db.users.distinct("city", {…})`, `insertOne/insertMany`,
`updateOne/updateMany`, `replaceOne`, `deleteOne/deleteMany`. Collections with odd names:
`db.getCollection("a.b")` or `db["a.b"]`. Anything else as a JSON database command run with
`runCommand`, e.g. `{ "collStats": "users" }` (cursor replies show their first batch). Keys may be
unquoted, strings single-quoted, and `ObjectId("…")`, `ISODate("…")`, `NumberLong(…)`,
`NumberDecimal("…")`, `UUID("…")` are understood (they become Extended JSON).

## Not built yet (from the spec)
Row editing / "+ Row" / column add (IDE-014), column and index details in the tree, syntax highlighting/autocomplete (gated on ADR-003 spike),
query history (IDE-013), CSV/JSON export (IDE-017), transactions UI, column resize/sort/copy, SSH, packaging
and signing, Redis, AI. MongoDB: no `explain`, change streams, GridFS or regex literals (`/x/i`; use
`{ $regex: "x", $options: "i" }`). SurrealDB: no `LIVE SELECT` streaming, no server-side cancel. The grid shows at most `max_rows` (default 10,000).

## Prerequisites
- Rust via rustup. `rust-toolchain.toml` installs **1.99.1** automatically. SQLx 0.9
  needs ≥ 1.94.
- macOS: Xcode command line tools (Metal shaders are compiled at build time). Linux: the
  GPUI system packages listed in `../ide/script/linux` (Vulkan, Wayland/X11, fontconfig).
- Build from the monorepo root (`../`): the workspace and Cargo aliases live there, and GPUI
  is a path dependency into `../ide`.
- Optional: Docker (local PostgreSQL, MySQL, SurrealDB, MongoDB for integration tests).

## Commands
```bash
cargo desktop                    # run the app (alias for: cargo run -p ferrumdata-desktop)
cargo test --workspace           # unit + service tests
cargo lint                       # clippy, warnings are errors
cargo fmt --all

docker compose up -d postgres    # local PG on 127.0.0.1:54329 (ferrumdata / ferrumdata-dev-password, db ferrumdata_test)
FERRUMDATA_TEST_PG=1 cargo test -p ferrumdata-db-postgres --test postgres
docker compose up -d mysql surrealdb mongodb   # MySQL :33069, SurrealDB :18000 (root / ferrumdata-dev-password), MongoDB :27019
FERRUMDATA_TEST_MYSQL=1 cargo test -p ferrumdata-db-mysql --test mysql
FERRUMDATA_TEST_SURREAL=1 cargo test -p ferrumdata-db-surrealdb --test surrealdb
FERRUMDATA_TEST_MONGO=1 cargo test -p ferrumdata-db-mongodb --test mongodb
cargo test -p ferrumdata-db-sqlite              # temp-file databases, no server needed

cargo test -p ferrumdata-credentials -- --ignored   # real OS keychain round trip (may prompt)
RUST_LOG=debug cargo desktop                     # verbose logs (never include SQL or secrets)
```

### Styling
There is no CSS. Views are GPUI elements styled with Tailwind-like builder methods and
colours from the active theme (`cx.theme().colors()`). Shared widgets (text input, splitter,
modal, notice, toolbar/status bar) come from `ferrum-ui` (`../shared/ui`); buttons, labels,
icons, menus and tooltips come from FerrumIDE's `ui` crate. Fonts and icons are embedded
from `../ide/assets`.

### Packaging
Bundling, signing and notarization are **not yet set up** for the GPUI build (ADR-009,
IDE-018).

## Layout
```
apps/desktop/            GPUI app: root view (app.rs), components/{shell,connection,explorer,sql_editor,result_grid,table_view,dialogs}, state/, commands/
crates/domain/           pure types: profiles, Secret, CellValue, QueryEvent, QueryState
crates/db-driver-api/    DatabaseDriver / DbConnection traits, bounded event channel, cancellation, document flattening
crates/db-postgres/      SQLx PostgreSQL adapter (streaming, cancel, exact NUMERIC decoding)
crates/db-mysql/         SQLx MySQL/MariaDB adapter (streaming, KILL QUERY cancel)
crates/db-sqlite/        SQLx SQLite adapter (database files, progress-handler interrupt)
crates/db-surrealdb/     SurrealDB adapter (official SDK, remote ws/wss only, SurrealQL)
crates/db-mongodb/       MongoDB adapter (official driver, shell-subset/JSON-command input)
crates/application/      AppServices runtime, ConnectionService, QueryService, safety heuristics
crates/persistence/      SQLite app state + migrations (settings, profiles, tabs)
crates/credentials/      OS keychain (keyring 4) + session-memory fallback
crates/editor-core/      statement splitter, cursor → statement, UTF-16 ↔ byte offsets
crates/result-grid-core/ viewport math, bounded ResultBuffer
docs/                    ARCHITECTURE, SECURITY, DRIVER_CONTRACT, QA_TEST_PLAN, ADR/
```

## Local data
`~/Library/Application Support/dev.FerrumData.FerrumData/ferrumdata.db` on macOS (OS-appropriate
elsewhere via `directories`). Deleting it resets FerrumData. Saved passwords live in the OS
keychain under the service `dev.ferrumdata.connection`.
