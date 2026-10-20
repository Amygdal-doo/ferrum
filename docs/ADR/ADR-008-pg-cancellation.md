# ADR-008: PostgreSQL cancellation semantics

Status: Accepted · 2026-10-18

SQLx 0.9 exposes no protocol-level cancel API. On cancel, timeout, client row limit, or
when the consumer drops the stream, the adapter:

1. Records `pg_backend_pid()` when the session is leased, before executing.
2. Stops polling the stream immediately and acknowledges with `QueryEvent::Cancelled`
   (or `TimedOut`).
3. Opens a short-lived **side connection** and calls `pg_cancel_backend(pid)`. It does
   not use the pool, which may be exhausted.
4. Detaches and closes the leased session instead of returning a possibly-busy
   connection to the pool.

Verified by `crates/db-postgres/tests/postgres.rs::cancel_stops_server_query_and_pool_recovers`,
which checks `pg_stat_activity` shows no surviving `pg_sleep` backend and that the pool
still serves queries afterwards. MySQL/SQLite need their own ADRs (`KILL QUERY`,
`sqlite3_interrupt`).
