# Architecture

One desktop process. No backend server. Same shape as FerrumData.

```
apps/desktop (Dioxus UI, UI thread)
   │  commands::background(...) ──► AppServices::spawn (dedicated Tokio runtime, 2 workers)
   ▼
crates/application   AppServices · prepare (auth inheritance + variable layers) · send (+history) · import/export
   │                      │                        │
   ▼                      ▼                        ▼
crates/http          crates/persistence       crates/interop
 (reqwest + rustls,   (SQLite app state,       (Postman v2.1, cURL import,
  substitution,        migrations)              code generation)
  cancellation)
   ▲                      ▲                        ▲
crates/domain  ◄── every crate (pure serde types; no UI, no I/O)
```

## Dependency rules (enforced by Cargo manifests)
- `domain` depends on nothing internal, and on neither Dioxus, reqwest nor SQLx.
- `http`, `persistence`, `interop` depend on `domain` only — never on each other.
- `application` wires them together and never depends on Dioxus.
- `desktop` depends on `application` (which re-exports the other crates) and `domain`.

## Request lifecycle
```
tab.request (authored, with {{vars}}, auth may be Inherit)
  └─ application::prepare(request, origin, collections, globals, env)
       • Inherit → nearest folder/collection auth (tab's unsaved auth respected)
       • VariableSet = globals < collection variables < active environment
  └─ AppServices::send(authored, prepared, origin, cancel)      [services runtime]
       └─ http::HttpClient::execute(prepared.request, vars, cancel)
            substitute → build URL/headers/auth/body → reqwest (cached client per
            verify_tls × follow_redirects) → select! with CancellationToken
       └─ history row (authored request + status/time/size or error; not on cancel)
  └─ UI shows HttpResponse (in memory only, per tab)
```

### URL ↔ params invariant
`HttpRequest.url` is the source of truth for the query string. `params` mirrors the
enabled pairs in the URL and additionally keeps disabled rows. `domain::urlparams`
implements both directions without percent-encoding, so `{{vars}}` survive editing.
The HTTP layer therefore never re-appends `params`.

### Variables
`{{name}}` substitution is single-pass (a value containing `{{x}}` is not expanded
again), unknown names are left verbatim and reported as unresolved for highlighting.

## Local state
`~/Library/Application Support/dev.FerrumAPI.FerrumAPI/ferrumapi.db` (platform equivalent
elsewhere via `directories`), SQLite with WAL, migrations in `crates/persistence/migrations`
(embedded via `sqlx::migrate!`). Collections, environments and history entries are JSON
documents keyed by UUID; tabs and settings are single JSON rows. Response bodies are never
persisted.

## Secrets
Auth credentials and secret variables are stored locally in the SQLite file (like
Postman's local workspace). They are never logged: `Auth` and `Variable` implement a
redacting `Debug`, logs carry only ids, methods, status codes and sanitized errors, and
reqwest errors are stripped of URLs before display.
