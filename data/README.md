# FerrumData

A local-first, DataGrip-class database IDE written in Rust: GPUI desktop (the UI framework
from Zed, a pinned git dependency) with the shared `ferrum-ui` crate, SQLx. No backend server,
no webview, no React/TypeScript/Tauri/Electron. Part of the Ferrum monorepo (`../`).

> **Status: M0/M1 foundation + PostgreSQL, MySQL, SQLite, SurrealDB and MongoDB drivers, plus a Redis key browser (alpha).**
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
| SQL editor: syntax highlighting per dialect (PostgreSQL dollar quotes / `E''` / `$1`, MySQL backticks / `#` / backslash escapes, SQLite `[ident]`); completion while typing or on ⌃Space / ⌘I: keywords and functions, tables after `FROM`/`JOIN`/`UPDATE`/`INTO`, columns of the tables the statement names, `alias.` → that table's columns (↑ ↓ ⏎ ⇥ Esc). Tables and columns load lazily from the active connection | ✅ lexer and completion unit-tested (`editor-core`); popup compiles, not yet manually smoke tested |
| Row editing (IDE-014) in table tabs of PostgreSQL / MySQL / SQLite tables with a primary key: double-click a cell to edit (Enter keeps, Esc cancels), **+ Row** (column defaults until edited), **Delete** marks the selected rows, **Set NULL**. Changes are staged (tinted cells and rows) and listed as the exact SQL Apply runs; **Apply** runs them in one transaction, each statement must touch exactly one row (else everything is rolled back), **Discard** drops them. Production connections confirm first; read-only profiles refuse (UI, service and read-only session). Only `SELECT * FROM <table> [WHERE/ORDER BY/LIMIT]` results are editable | ✅ statements unit-tested; applied end to end on SQLite (incl. rollback on a stale row and read-only refusal); scripts live-tested on PostgreSQL 17 and MySQL 8.4; grid editing UI compiles, not yet manually smoke tested |
| Query history (IDE-013): every run recorded per connection (text, time, duration, rows or error, incl. failed/cancelled runs and table browsing) in the local app DB, max 2,000 per connection; **History** tab in the results panel with search, this connection / all connections, Open (new tab), Run (new tab + run, production confirmation applies), double-click to open, Clear… | ✅ store + service unit-tested; panel compiles, not yet manually smoke tested |
| Export (IDE-017): **Export → CSV / JSON…** saves every loaded row (not just the visible ones, in the current sort order) via a save dialog; **Copy as → CSV / JSON / Markdown** copies the selection, or all rows when nothing is selected | ✅ serializers unit-tested (quotes, newlines, NULL vs `''`, binary, decimals, JSON columns, absent document fields); dialog/clipboard not yet manually smoke tested |
| Grid: click a header to sort (asc → desc → off; client-side, type-aware: numbers by value incl. exact decimals, dates/timestamps chronologically, text case-insensitively, NULL/absent last), drag a header's right edge to resize, click/shift-click/row-number selection, ⌘C copies a cell (full value) or a range (TSV), ⌘A selects all | ✅ comparator, selection and copy logic unit-tested; mouse/keyboard wiring not yet manually smoke tested |
| Explorer: expand a PostgreSQL / MySQL / SQLite table (chevron) to see its columns (type, nullable, default, PK) and indexes (unique, primary, key columns or expressions) | ✅ live-tested (PG 17, MySQL 8.4, SQLite); tree UI not yet manually smoke tested |
| Redis profiles (alpha): host + port **or** a `redis://` / `rediss://` URL in the Host field, ACL user + password (keychain), DB index, TLS mode, read-only flag | ✅ profile validation and connection settings unit-tested; connect/auth live-tested (Redis 7.4); form compiles, not yet manually smoke tested |
| Redis key explorer: expand the connection → pattern box (`SCAN … MATCH`, never `KEYS`), 200-key pages with **Load more**, loaded / `DBSIZE` count, keys grouped into a `:` tree (lazy folders with counts), type badge and TTL per key | ✅ SCAN paging + `TYPE`/`PTTL` pipeline live-tested; tree grouping unit-tested; tree UI compiles, not yet manually smoke tested |
| Redis value viewer (key tab): string (pretty JSON when it is an object/array; > 1 MiB shown truncated), hash (field/value), list (index/value), set (member), sorted set (member/score), stream (id + one column per field), all in the results grid (sort, copy, export); summary with length, TTL and `MEMORY USAGE` | ✅ every type live-tested at the adapter; key tab UI compiles, not yet manually smoke tested |
| Redis edits: save a string value (`SET … KEEPTTL`), set TTL (`90`, `15m`, `2h`, `1d`) / remove TTL (`PERSIST`), delete key (confirmation dialog); disabled in read-only profiles and refused by the adapter | ✅ live-tested at the adapter (incl. read-only refusal); buttons/dialog compile, not yet manually smoke tested |
| Redis command console: an editor tab on a Redis connection runs the line under the caret (redis-cli quoting); replies map to the grid (scalars → `value`, arrays → `#`/`value`, maps and `HGETALL`/`CONFIG GET`/`… WITHSCORES` → two columns, `INFO` → section/field/value, nested → JSON); recorded in the query history | ✅ live-tested (incl. `WRONGTYPE` errors, row limit, cancel and timeout of a blocking `BLPOP`) |

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
| Redis 7 (alpha; Valkey untested) | `db-redis` | one Redis command per run (redis-cli quoting) | ✅ live (Redis 7.4 with `requirepass`: bad password, unreachable port); TLS (rustls) compiled and unit-tested only, **not live-tested** | ✅ live; UTF-8 as text, other bytes as binary, nil as `NULL`, integers/doubles typed | ✅ live (reply cut at `max_rows`) | ✅ client-side: the run's own connection is dropped, which also unblocks the server (live-tested with `BLPOP`) | ✅ live (client) | ⚠️ explicit read allow-list (UX guard; use an ACL user for real protection) | n/a: key explorer instead (`SCAN`, live) |

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

**Redis console and read-only.** Each console run uses its own connection, so `SELECT`, `MULTI`/`EXEC`,
`WATCH`, `AUTH`/`HELLO` and streaming commands (`SUBSCRIBE`, `MONITOR`) are refused with an explanation (set the
DB index and credentials in the profile). On read-only profiles only commands on an explicit allow-list run
(`GET`, `HGETALL`, `SCAN`, `TYPE`, `TTL`, `INFO`, `CONFIG GET`, `XRANGE`, `MEMORY USAGE`, …; see
`crates/db-redis/src/command.rs`); anything else, including unknown commands, is refused with the command name.
On production-tagged profiles the same list decides when to ask for confirmation.

## Not built yet (from the spec)
Column add (IDE-014), multi-cursor and undo/redo in the editor,
transactions UI, SSH, AI. Redis (alpha) does not yet have: editing of hash/list/set/
sorted-set/stream elements (only string values, TTL and delete), pub/sub, `MONITOR`, transactions, Cluster/Sentinel,
RESP3, key rename/create, opening keys that are not valid UTF-8 (they are listed but disabled), key tabs restored
after a restart, a live-tested TLS connection, or separate "verify CA only" (both verify modes check chain and host). Column/index details for SurrealDB and MongoDB
(SQL engines only), foreign keys / constraints / triggers in the tree. Export of more than the loaded rows
(it never re-runs the query: a result cut at `max_rows` exports those rows), Excel/SQL-INSERT export, persisted
column widths and sort, keyboard navigation of the grid selection, history paging beyond the newest 200 matches. MongoDB: no `explain`, change streams, GridFS or regex literals (`/x/i`; use
`{ $regex: "x", $options: "i" }`). SurrealDB: no `LIVE SELECT` streaming, no server-side cancel. The grid shows at most `max_rows` (default 10,000).

## Prerequisites
- Rust via rustup. `rust-toolchain.toml` installs **1.99.1** automatically. SQLx 0.9
  needs ≥ 1.94.
- macOS: Xcode command line tools (Metal shaders are compiled at build time). Linux: the
  GPUI system packages listed in `../packaging/linux/Dockerfile` (Vulkan, Wayland/X11, fontconfig).
- Build from the monorepo root (`../`): the workspace and Cargo aliases live there, and GPUI
  is a git dependency on upstream Zed, pinned in the root `Cargo.toml`.
- Optional: Docker (local PostgreSQL, MySQL, SurrealDB, MongoDB, Redis for integration tests).

## Commands
```bash
cargo desktop                    # run the app (alias for: cargo run -p ferrumdata-desktop)
cargo test --workspace           # unit + service tests
cargo lint                       # clippy, warnings are errors
cargo fmt --all

docker compose up -d postgres    # local PG on 127.0.0.1:54329 (ferrumdata / ferrumdata-dev-password, db ferrumdata_test)
FERRUMDATA_TEST_PG=1 cargo test -p ferrumdata-db-postgres --test postgres
docker compose up -d mysql surrealdb mongodb   # MySQL :33069, SurrealDB :18000 (root / ferrumdata-dev-password), MongoDB :27019
docker compose up -d redis                     # Redis 7.4 :63799 (password ferrumdata-dev-password, no persistence)
FERRUMDATA_TEST_MYSQL=1 cargo test -p ferrumdata-db-mysql --test mysql
FERRUMDATA_TEST_SURREAL=1 cargo test -p ferrumdata-db-surrealdb --test surrealdb
FERRUMDATA_TEST_MONGO=1 cargo test -p ferrumdata-db-mongodb --test mongodb
FERRUMDATA_TEST_REDIS=1 cargo test -p ferrumdata-db-redis --test redis
cargo test -p ferrumdata-db-sqlite              # temp-file databases, no server needed

cargo test -p ferrumdata-credentials -- --ignored   # real OS keychain round trip (may prompt)
RUST_LOG=debug cargo desktop                     # verbose logs (never include SQL or secrets)
```

### Styling
There is no CSS. Views are GPUI elements styled with Tailwind-like builder methods and
colours from the active theme (`cx.theme().colors()`). Shared widgets (text input, splitter,
modal, notice, toolbar/status bar, buttons, labels, icons, menus and tooltips) come from
`ferrum-ui` (`../shared/ui`), which also embeds the fonts (IBM Plex Sans, Lilex) and the
Lucide icons.

### Packaging
FerrumData is bundled with `scripts/bundle-macos.sh` like the other apps; released macOS
builds are signed with the Developer ID and notarized, and installed through FerrumLauncher
(see [INSTALL.md](../INSTALL.md)).

## Layout
```
apps/desktop/            GPUI app: root view (app.rs), components/{shell,connection,explorer,sql_editor,result_grid,history,table_view,key_view,dialogs}, state/, commands/
crates/domain/           pure types: profiles, Secret, CellValue, QueryEvent, QueryState
crates/db-driver-api/    DatabaseDriver / DbConnection traits, bounded event channel, cancellation, document flattening
crates/db-postgres/      SQLx PostgreSQL adapter (streaming, cancel, exact NUMERIC decoding)
crates/db-mysql/         SQLx MySQL/MariaDB adapter (streaming, KILL QUERY cancel)
crates/db-sqlite/        SQLx SQLite adapter (database files, progress-handler interrupt)
crates/db-surrealdb/     SurrealDB adapter (official SDK, remote ws/wss only, SurrealQL)
crates/db-mongodb/       MongoDB adapter (official driver, shell-subset/JSON-command input)
crates/db-redis/         Redis/Valkey adapter (redis-rs + rustls): SCAN key browser, value views, console, read allow-list
crates/application/      AppServices runtime, ConnectionService, QueryService, safety heuristics
crates/persistence/      SQLite app state + migrations (settings, profiles, tabs, query history)
crates/credentials/      OS keychain (keyring 4) + session-memory fallback
crates/editor-core/      statement splitter, cursor → statement, UTF-16 ↔ byte offsets
crates/result-grid-core/ viewport math, bounded ResultBuffer, sort comparator, selection, CSV/JSON/Markdown export
docs/                    ARCHITECTURE, SECURITY, DRIVER_CONTRACT, QA_TEST_PLAN, ADR/
```

## Local data
`~/Library/Application Support/dev.FerrumData.FerrumData/ferrumdata.db` on macOS (OS-appropriate
elsewhere via `directories`). Deleting it resets FerrumData. It holds settings, profiles, editor
tabs and the query history (SQL text, never row values). Saved passwords live in the OS
keychain under the service `dev.ferrumdata.connection`.
