# QA test plan

## Automated
| Layer | Command | Notes |
|-------|---------|-------|
| Unit + service tests | `cargo test --workspace` | No external services |
| PostgreSQL contract | `docker compose up -d postgres` then `FERRUM_TEST_PG=1 cargo test -p ferrum-db-postgres --test postgres` | CI runs this on Ubuntu |
| OS keychain | `cargo test -p ferrum-credentials -- --ignored` | Touches the real keychain; may prompt |
| Lint | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings` | |

## Manual UI smoke checklist (run after UI or vendored-component changes)
1. Fresh start with no data dir: window opens, one empty "SQL 1" tab, empty sidebar with "Add PostgreSQL connection".
2. Create a profile (`docker compose` server: `127.0.0.1:54329`, `ferrum` / `ferrum-dev-password`, db `ferrum_test`, TLS Disabled). **Test connection** shows the version. A wrong password shows an auth error without the password.
3. Save with "Save password in keychain" on. Keychain Access shows `dev.ferrum.connection`. `ferrum.db` contains no password (`sqlite3 ferrum.db "select data from connection_profiles"`).
4. Type `select * from generate_series(1, 100000);` and press ⌘↵. Rows stream in, the 10,000-row limit notice appears, and scrolling stays smooth.
5. Run `select pg_sleep(30);` and press **Cancel**: the UI acknowledges immediately and the status shows "Cancelled".
6. Two statements in one tab: caret in the second plus ⌘↵ runs only the second. Selecting text runs only the selection.
7. Mark the profile Production and run `delete from x`: a confirmation dialog appears, and Cancel runs nothing.
8. Drag both splitters, switch theme, quit, relaunch: layout, theme, tabs and SQL text are restored.
9. Stop the database mid-session and run a query: there's an error in the results panel and no crash.
10. "Load 100k synthetic rows": scroll top to bottom and check memory in Activity Monitor (record in ADR-004).
11. IME: type Japanese via the macOS input method in the editor (record in ADR-003).
