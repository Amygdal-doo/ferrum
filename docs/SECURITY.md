# Security model

## Secrets
- Passwords live only in the OS vault or in process memory (`SessionStore`); see ADR-007.
- `ferrum_domain::Secret` has a redacting `Debug`, no `Display` and no `Serialize`.
- `ConnectionProfile` stores a `SecretRef` (service + account), never secret material.
  A test asserts the serialized profile contains no `password` field.
- **Known upstream hazard:** SQLx's `PgConnectOptions` prints the password in its `Debug`
  output. The adapter only holds it inside `PgOptions`, whose `Debug` is redacted. A canary
  test fails if upstream behaviour changes.
- Driver error text is scrubbed of the password before reaching UI or logs
  (`Secret::scrub`, tested with an auth failure against a real server).

## Logging
`tracing` logs ids, states and sanitized errors only. No SQL text, row values or
credentials. Default filter: `warn,ferrum_*=info`; override with `RUST_LOG`.

## Database safety
- Read-only profiles set `default_transaction_read_only=on` for the session (server
  enforced; verified by test with SQLSTATE 25006). A session can override it, so it is
  **not a security boundary**. Use read-only database roles for real protection.
- Production-tagged profiles get a confirmation dialog for statements that may mutate
  data. This is a keyword heuristic (`application::safety`), a UX guard only, never a
  security control.
- User SQL is executed verbatim through `AssertSqlSafe`: Ferrum is a SQL client. Any SQL
  Ferrum generates itself (row editing, IDE-014) must use bind parameters and dialect
  identifier quoting.

## TLS
Default for new profiles in the domain model is `VerifyFull`. The form preselects `Prefer`
for local development and labels unverified modes ("Server certificate is not verified").
Client certificates and custom CAs are P1.

## Rendering
Cell values render as Dioxus text nodes; nothing from a database is interpreted as HTML.

## Supply chain
Toolchain pinned (`rust-toolchain.toml`), `Cargo.lock` committed, Dioxus pinned exactly,
Rust/UI vendored at a commit, weekly `cargo audit` workflow.
