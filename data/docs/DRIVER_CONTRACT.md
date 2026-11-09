# Driver contract

Every adapter implements `ferrumdata_db_driver_api::{DatabaseDriver, DbConnection}`.

## Rules
1. `connect` builds a pool and proves it with a round trip. Errors are `DbError`s with a
   meaningful `kind` (`Authentication`, `Connection`, `Timeout`, …) and no secrets.
2. `execute` returns immediately with a bounded event channel (`event_channel()`) and a
   `CancellationHandle`. Work happens in a task that leases one session for its whole life.
3. Event order: `Started`, then optionally `Metadata` and `Rows*` / `Message*`, then
   **exactly one** terminal event (`Completed`, `Cancelled`, `TimedOut`, `Failed`).
4. Statements without a result set complete with `rows_affected`.
5. `limits.max_rows` stops fetching; `Completed.truncated = true`. Never rewrite user SQL.
6. `limits.timeout` produces `TimedOut` and cancels server-side work.
7. Cancel: acknowledge promptly, cancel server-side when the engine allows, and never
   return a possibly-busy session to the pool.
8. If the receiver is dropped, treat it as cancel.
9. `NULL` ≠ empty string. Decimals are exact strings. Timestamps keep their offset.
   Undecodable values become `CellValue::Unsupported(type_name)`, never a panic.
10. `list_tables` returns user objects ordered by schema, name; names exactly as stored
    (callers quote them with `DatabaseEngine::quote_ident`).
11. Capabilities a driver lacks are reported via `CapabilitySet`, not by failing at runtime.

## Contract tests (PostgreSQL: `crates/db-postgres/tests/postgres.rs`)
| Behaviour | Test |
|-----------|------|
| Connect + version | `connects_and_reports_version` |
| Auth failure kind + no secret echo | `bad_password_is_authentication_error_without_secret` |
| Type mapping incl. NULL vs '', Unicode, NUMERIC scale/NaN, bytea, tz, json, uuid | `decodes_types_and_distinguishes_null_from_empty` |
| Batching, max_rows truncation, pool reuse | `streams_in_batches_and_truncates_at_max_rows` |
| Cancel + server-side stop + pool recovery | `cancel_stops_server_query_and_pool_recovers` |
| Timeout | `timeout_is_reported` |
| DDL/DML affected rows, structured SQLSTATE | `ddl_and_dml_report_affected_rows_and_errors_are_structured` |
| Read-only session refuses writes | `read_only_profile_rejects_writes` |
| Table listing: case-preserved names, composite PK order, views, no system schemas | `lists_tables_with_case_preserved_names_and_primary_keys` |

Equivalent suites: `crates/db-mysql/tests/mysql.rs` (`FERRUMDATA_TEST_MYSQL=1`),
`crates/db-sqlite/tests/sqlite.rs` (always runs, temp files), `crates/db-surrealdb/tests/surrealdb.rs`
(`FERRUMDATA_TEST_SURREAL=1`) and `crates/db-mongodb/tests/mongodb.rs` (`FERRUMDATA_TEST_MONGO=1`).
MySQL error `code`s are MySQL error numbers (e.g. `1062`) with the SQLSTATE in `detail`;
SQLite codes are extended result codes; MongoDB codes are server error codes (e.g. `11000`);
SurrealDB codes are the SDK error kind (`Validation`, `NotFound`, …).

## Document engines (SurrealDB, MongoDB)
- Rule 2/3 hold, but results are buffered (≤ `max_rows` documents) before `Metadata`, so the
  columns are the union of all fetched documents' top-level fields (`documents::DocumentTable`).
  A missing field is `CellValue::Unsupported("absent")`, never `Null`.
- Rule 4: writes complete with `rows_affected` (MongoDB) or return the written records as rows
  (SurrealDB, as the server does).
- Rule 6/7: MongoDB passes the timeout as `maxTimeMS` and kills cursors on drop. SurrealDB has
  no query kill: cancel/timeout drop the result stream and acknowledge promptly, but the server
  may finish the statement (`query_cancel: false` in its `CapabilitySet`).
- Read-only profiles: neither engine has a read-only session, so the driver refuses writing
  statements before sending them (keyword/method based; a UX guard, not a security boundary).
