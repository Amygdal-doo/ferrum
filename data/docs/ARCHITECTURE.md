# Architecture

One desktop process. No backend server.

```
apps/desktop (GPUI UI, main thread; widgets from ferrum-ui + Zed `ui`)
   │  commands::background(...) ──► AppServices::spawn (dedicated Tokio runtime, 2 workers)
   ▼
crates/application   ConnectionService · QueryService (supervisor) · safety
   │ traits                     │                      │
   ▼                            ▼                      ▼
crates/db-driver-api      crates/persistence     crates/credentials
   ▲                       (SQLite app state)     (OS keychain / session memory)
crates/db-postgres (SQLx)
   ▲
crates/domain  ◄── every crate (pure types; no UI, no SQLx)
crates/result-grid-core, crates/editor-core  (pure, UI-agnostic models)
```

## Dependency rules (enforced by Cargo manifests)
- `domain` depends on nothing internal, and on neither GPUI nor SQLx.
- `db-*` adapters depend on `domain` and `db-driver-api` only.
- `application` depends on traits and adapters, never on GPUI.
- `desktop` depends on `application`, `domain` and the pure model crates.

## Threading
- The main thread runs GPUI (one root `FerrumData` view owns all UI state; commands are
  methods on it and update it via `cx.spawn` + `WeakEntity::update`). All DB, disk and
  keychain I/O runs on the services runtime
  owned by `AppServices`. Keychain calls go through `spawn_blocking`.
- UI tasks await `JoinHandle`s and `tokio::sync::mpsc` receivers, which work from any
  executor.
- Shutdown: the runtime is shut down in the background (`RuntimeGuard`), because dropping
  it inside the UI's async context would panic.

## Query lifecycle
`QueryState`: Created → Running → Streaming → Completed | Cancelled | Failed | TimedOut.
`QueryState::apply` rejects invalid transitions (unit-tested); late events after a
terminal state are dropped.

```
Driver task ── bounded mpsc(4) ──► QueryService supervisor ── bounded mpsc(4) ──► UI task
  (leases one session,              (stamps execution_id +       (drops events whose
   batches of 250 rows,              monotonically increasing     execution_id is no longer
   stops at max_rows)                seq, guarantees exactly      the tab's current run)
                                     one terminal event)
```
Backpressure: the driver awaits `send` before pulling more rows from the socket, so rows in
flight are bounded at roughly 2 × 4 × batch size. When the UI drops its receiver (new run,
tab closed), the supervisor cancels the driver.

## Local state
`~/Library/Application Support/dev.FerrumData.FerrumData/ferrumdata.db` (platform equivalent elsewhere),
SQLite with WAL, migrations in `crates/persistence/migrations` (embedded via
`sqlx::migrate!`). Tables: `settings` (versioned JSON with serde defaults),
`connection_profiles` (JSON, no secrets), `editor_tabs` (autosaved every 1.5 s when changed)
and `query_history` (one row per run, written by `QueryService` before the terminal event is
forwarded; query text and a result summary, never row values; pruned to 2,000 entries per
connection and deleted with the connection).
