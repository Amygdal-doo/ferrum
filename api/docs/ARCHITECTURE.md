# Architecture

One desktop process. No backend server. Same shape as FerrumData.

```
apps/desktop (GPUI UI, main thread; root view `FerrumApi` owns `AppState`)
   │  cx.spawn + commands::run(...) ──► AppServices::spawn (dedicated Tokio runtime, 2 workers)
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
- `domain` depends on nothing internal, and on neither GPUI, reqwest nor SQLx.
- `http`, `persistence`, `interop`, `server` depend on `domain` only — never on each other.
- `application` wires them together and never depends on GPUI.
- `desktop` depends on `application` (which re-exports the other crates), `domain`, and the
  UI stack: `ferrum-ui` plus GPUI / `ui` from `ide/`.

## Request lifecycle
```
tab.request (authored, with {{vars}}, auth may be Inherit)
  └─ application::prepare(request, origin, collections, globals, env)
       • Inherit → nearest folder/collection auth (tab's unsaved auth respected)
       • VariableSet = globals < collection variables < active environment
  └─ AppServices::send(authored, prepared, origin, cancel)      [services runtime]
       └─ cookie jar → Cookie header for the resolved URL (user Cookie header wins per name)
       └─ http::HttpClient::execute(prepared.request, vars, cancel)
            substitute → build URL/headers/auth/body → reqwest (cached client per
            verify_tls × follow_redirects) → select! with CancellationToken
       └─ Set-Cookie of the response → cookie jar (stored against the final URL, saved to SQLite)
       └─ history row (authored request + status/time/size or error; not on cancel)
  └─ UI shows HttpResponse (in memory only, per tab)
```

The collection runner (`application::runner`) plans saved requests of a collection or
folder in tree order (`plan_run`, same `prepare` as a tab), then `run_collection` executes
them one by one through the same cookie-jar path, without history, streaming `RunEvent`s
over a channel that the UI drains into `AppState::runner`. Stop cancels the shared token.

## Local tools
- **Mock server** — `domain::mock` derives a route path from each saved request's URL and
  picks the most specific route for an incoming request (method, path params, query/header
  conditions). `application::plan_mock_routes` builds routes from the collection plus its
  `CollectionMocks` (stored in the `mocks` table, one JSON row per collection, FK-cascaded).
  `server::MockServer` (hyper, 127.0.0.1 only) holds the routes behind an `RwLock`, so UI
  edits swap them live, and streams `MockLogEntry`s over a channel into `AppState::mocks`.
- **Webhook inspector** — `server::WebhookInspector` pushes every request into a shared
  `domain::webhook::CaptureLog` (ring buffer, 500) and notifies the UI with the capture id;
  the UI coalesces notifications and re-reads the log. `CapturedRequest::to_request` turns a
  capture into an `HttpRequest` for replay. Nothing is persisted.
- **Load test** — `application::run_load` drives `exec` through `buffer_unordered(C)` over
  `0..N`, stopping new requests on cancel or the duration limit, and reports a cloned
  `LoadTally` every 200 ms; `AppServices::run_load_test` plugs in `HttpClient::execute`
  (no cookie jar, no history). Statistics (`domain::load`) are computed from the tally.

Servers are owned by UI state: dropping a `MockServer` / `WebhookInspector` handle cancels
its accept loop and open connections.

### URL ↔ params invariant
`HttpRequest.url` is the source of truth for the query string. `params` mirrors the
enabled pairs in the URL and additionally keeps disabled rows. `domain::urlparams`
implements both directions without percent-encoding, so `{{vars}}` survive editing.
The HTTP layer therefore never re-appends `params`.

### Variables
`{{name}}` substitution is single-pass (a value containing `{{x}}` is not expanded
again), unknown names are left verbatim and reported as unresolved for highlighting.

## UI state
The root GPUI view (`app.rs`, `FerrumApi`) owns a plain `AppState` and renders the whole
window from it; mutations happen through `cx.listener` / `cx.spawn` and end in
`cx.notify()`. Text fields are the only stateful widgets: `components/fields.rs` keeps one
`ferrum_ui::TextInput` entity per field key, re-syncs controlled fields from state on every
render (so URL ↔ params edits show up in the other field) and drops fields a render no
longer uses. Dialogs keep their working copy inside `state::Dialog` until Save.

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
