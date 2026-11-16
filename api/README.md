# FerrumAPI

A local-first, Postman-style API client written in Rust: a native GPUI desktop app (the UI
framework of Zed, a pinned git dependency) built on the shared `ferrum-ui` crate,
reqwest + rustls, SQLite. No backend server, no account, no Electron, no webview. Part of
the Ferrum monorepo — sibling of [FerrumData](../data), [FerrumGIT](../git) and
[FerrumIDE](../editor), with the same theme and components.

> **Status: M0 — first vertical slice.** See "What works" below; nothing beyond that list
> should be assumed. "Tested" means automated tests; the GPUI UI has only been smoke-launched
> (start → 8 s → quit, no errors, empty and existing local data), not clicked through by a
> human yet.

## What works today
| Area | State |
|------|-------|
| Member of the Ferrum monorepo workspace, pinned toolchain (1.99.1), workspace lints, `cargo api` / `cargo lint` aliases | ✅ |
| Request execution: GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS over reqwest + rustls on a dedicated Tokio runtime | ✅ integration-tested against a local mock server |
| Bodies: none, raw (JSON/Text/XML/HTML, default Content-Type), x-www-form-urlencoded, multipart form-data (text + file), binary file | ✅ integration-tested |
| Auth: none, Basic, Bearer, API Key (header or query), inherited from folder/collection (tab's unsaved auth respected) | ✅ tested (http + application) |
| Auth: **OAuth 2.0** — authorization code with PKCE (S256) through a loopback redirect `http://127.0.0.1:<port>/callback` (any free port, or a fixed one), client credentials, refresh tokens; client auth in the body or as Basic; header prefix. **Get new access token** opens the default browser; tokens are cached per auth and **per environment** (memory + OS keychain) and fetched / refreshed when a request needs one. **OAuth 1.0a** (HMAC-SHA1, query + form-body parameters signed, realm). **Digest** (RFC 7616 / 2617: MD5, MD5-sess, SHA-256, qop=auth; the 401 challenge is answered once with a fresh body). **AWS Signature v4** (region, service, session token; payload hashed, `UNSIGNED-PAYLOAD` for streamed multipart; S3 single-encodes the path). A user `Authorization` header still wins. Postman v2.1 import / export of all four; cURL code uses `--digest` / `--aws-sigv4` | ✅ published test vectors (Twitter OAuth 1, AWS SigV4 suite, RFC 2617 / 7616, RFC 7636 PKCE); signatures recomputed against a local server; OAuth 2 end to end against a local axum authorization server (client credentials, per-environment cache, authorization code + PKCE via the loopback redirect, refresh, forget) |
| Auth secrets (OAuth client / consumer / token secrets, Digest password, AWS secret key and session token) and OAuth 2 tokens live in the **OS keychain** (`keyring`, service `dev.ferrum.api.secret`, like FerrumData / FerrumGIT / FerrumChat): collections, tabs, history, Postman exports and `.ferrum/api` repo files hold only a `secret_ref`. Written by the autosave loop as you type (typed edits replace, loaded-but-empty auths only merge, so nothing is wiped by a request reopened from history) | ✅ serde never writes them (domain test), store round trip / merge / replace / per-environment tokens tested with an in-memory store; keychain itself not exercised by tests |
| Send / cancel (CancellationToken), timeout, follow redirects, verify TLS | ✅ integration-tested (cancel returns in ~1 s; redirect on/off; loop → error) |
| `{{variable}}` substitution in URL, params, headers, body and auth; globals < collection < environment layering; unresolved detection | ✅ tested |
| Response data: status, time, size, headers, cookies (Set-Cookie parsed), full body bytes | ✅ tested |
| Collections with nested folders: create, rename, duplicate, delete, save requests into them | ✅ domain tree ops + SQLite persistence tested; UI wired (not hand-tested) |
| Environments + globals CRUD, active-environment selector | ✅ persistence tested; UI wired (not hand-tested) |
| History of sent requests (authored request + status/time/size; pruned to 500), reopen in a tab | ✅ recorded end-to-end in tests; UI wired (not hand-tested) |
| Multiple request tabs, restored after restart (debounced autosave) | ✅ reducers unit-tested; autosave observed writing to SQLite on smoke launch |
| Postman Collection v2.1 import/export (v2.0 import), Postman environment import. GraphQL bodies import as GraphQL requests and export in Postman's `graphql` mode; SOAP / OData / tRPC export as the plain HTTP request they send (noted in the description, `{{variables}}` kept); gRPC / WebSocket requests are skipped and listed in the collection description | ✅ fixture + round-trip tested; file import/export end-to-end tested; kind mapping tested |
| cURL import (incl. Chrome "Copy as cURL", `$'…'`, `-sSL`, `-F`, `-u`, `-G`) | ✅ tested |
| **Import** recognises the format by content, from a file (**Choose file…**, any extension) or pasted into the Import dialog: Postman v2.1, **Insomnia** exports (v4 JSON or YAML: workspace → collection, request groups → folders with their auth, `metaSortKey` order, base-environment data → collection variables, `{{ _.x }}` → `{{x}}`, query parameters, JSON / text / XML / form / multipart / file / GraphQL bodies, Basic / Bearer / API key / Digest / OAuth 1 / OAuth 2 / AWS IAM auth; v5 YAML with nested items), **OpenAPI 3.0 / 3.1** (JSON or YAML: one request per operation in a folder per first tag, `{{baseUrl}}` from the first server with its variable defaults, `{id}` → `{{id}}` with example values as collection variables, query / header parameters (required ones enabled), request-body examples given or built from the schema (`$ref`, `allOf`, `oneOf`, formats, depth-limited), form / multipart bodies, security schemes → auth, and the expected responses with example bodies in each request's description) or a cURL command (opens a tab). Secrets in imported files go to the keychain. **Copy as cURL** on a saved request in the tree (variables and inherited auth resolved); the Code dialog has cURL / Python / fetch | ✅ Insomnia v4 / v5, OpenAPI (YAML petstore with refs, tags, security, bodies), schema examples and format detection tested; file / paste import end to end (secret kept out of SQLite); menu not hand-tested |
| Code generation: cURL, Python `requests`, JavaScript `fetch` (GraphQL / SOAP / OData / tRPC as the HTTP request they send); `grpcurl` for gRPC and `websocat` for WebSocket | ✅ snapshot + cURL round-trip tested; kind commands tested |
| Response body search (⌘F / Ctrl+F): highlight all matches, current-match highlight, count, next/previous (Enter, ⌘G / ⇧⌘G, buttons), match-case toggle, Esc closes; scrolls to the match. Works in Pretty and Raw | ✅ matching unit-tested (case folding, Unicode offsets, non-overlap, 10 000-match cap); UI wired (not hand-tested) |
| JSON syntax colouring in Pretty view (keys, strings, numbers, booleans, null) from the theme's syntax colours; the pretty text is cached per response instead of re-parsed every frame | ✅ tokenizer + span layering unit-tested; UI wired (not hand-tested) |
| Cookie jar: `Set-Cookie` cookies stored for the workspace (SQLite, migration `0002_cookies`), sent on later requests to matching domain/path (host-only vs `Domain`, default path, `Secure` only over https, `Expires`/`Max-Age` expiry, deletion via past expiry); an explicit `Cookie` header wins per name. **Cookies** button in the header lists cookies by domain with delete / clear domain / clear all | ✅ matching + expiry unit-tested, persistence tested, end-to-end tested against a mock (login cookie → authenticated request → clear); dialog wired (not hand-tested) |
| Collection runner: **Run…** on a collection or folder (tree menu) sends its saved requests one at a time in tree order (inherited auth, current variables, cookie jar); per-request status, code, time, pass (2xx) / fail / error, Stop, Run again, click a row to open the request; closing the dialog keeps the run going (status bar shows progress and reopens it). Runs are not added to history | ✅ sequencing + stop unit-tested, run plan tested, end-to-end tested against a mock; dialog wired (not hand-tested) |
| Mock server: **Mock server…** on a collection (tree menu). Each saved request can have several mock responses (status, headers, body, delay ≤ 60 s, enable toggle) with optional query-param / header conditions. Routes come from the request's method and URL path (a leading `{{baseUrl}}` or `scheme://host` is dropped, the query ignored); `:id`, `{id}` and `{{id}}` segments match any value. The most specific match wins (more literal segments, then more conditions, then order). Unmatched requests get a 404 JSON body listing every route. Start/Stop per collection on a chosen port (default 4010, **127.0.0.1 only**, clear error when the port is taken); edits apply to the running server immediately; live log of requests and which mock answered. Definitions are stored per collection (SQLite, migration `0003_mocks`, deleted with the collection) | ✅ matching/route paths unit-tested, persistence tested, server tested over real sockets (match, conditions, delay, 404 listing, live route swap, port-in-use); dialog wired (not hand-tested) |
| Webhook inspector: **Webhooks** in the header. Listens on 127.0.0.1 at a chosen port (default 9000), any path; records time, method, path, query, headers, body (pretty JSON when it parses; first 1 MB kept) and size; configurable reply (status, Content-Type, body; editable while running). List + detail, Clear, **Open as request** (new tab, hop-by-hop headers dropped, URL pointing back at the listener so it can be replayed). Captures live in memory only (last 500). Public webhooks need a tunnel (`cloudflared` / `ngrok`) — the dialog says so; none is built in | ✅ capture formatting, replay conversion and the 500-cap log unit-tested; listener tested over real sockets; dialog wired (not hand-tested) |
| Load test: **Load** next to Send. N requests (≤ 10 000) at C concurrency (≤ 100), optional duration limit (≤ 600 s), using the tab's request as Send would (inherited auth, variables; no cookie jar, no history). Live progress, requests/s, OK / HTTP-error / failed counts by status and error kind, latency min/avg/p50/p90/p99/max (nearest-rank) and a 12-bar histogram; Cancel; closing the dialog keeps it running (status bar reopens it). Non-localhost targets show a warning and the button reads "Start anyway" | ✅ percentiles, histogram, caps and runner (exact count, bounded concurrency, cancel, duration limit) unit-tested; end-to-end against a local mock server (2xx, 5xx, connection refused); dialog wired (not hand-tested) |
| Request kinds: every saved request has a kind — HTTP/REST, GraphQL, SOAP, OData, tRPC, WebSocket, gRPC. Older records (no `kind`) load as REST; SQLite migration `0004_projects_kinds` adds the project column and a schema cache. Collections / folders hold any mix of kinds; the tree and tab strip show the method (REST) or a kind badge (GQL, SOAP, ODATA, TRPC, WS, GRPC). **Add request…** on a collection / folder picks the kind; the chevron next to **+** in the tab strip opens a new tab of any kind (or the webhook inspector). Each kind keeps its own settings, so switching kinds loses nothing | ✅ domain (serde defaults, lowering), persistence (old-database upgrade test), mixed-kind round trip tested; headless GPUI render of every kind / section tested; not hand-tested |
| `{{variables}}`, inherited auth, cookie jar, history, runner and timeouts apply to every HTTP-based kind (GraphQL, SOAP, OData, tRPC): each is "lowered" to the plain HTTP request it sends after substitution. WebSocket and gRPC use the same headers / auth (as handshake headers / metadata) and variables in URL, metadata and messages | ✅ end-to-end tested against a local axum server (env variables + collection-inherited Bearer auth for each kind) |
| Projects: the shared Ferrum project bar now switches projects (recent projects from every Ferrum app, folder → git root). Collections belong to a project or are shared; the sidebar shows **Project · name** and **No project / shared** groups (with no project open: shared plus one group per project). New collections go into the current project; **Move to current project** / **Make shared** on a collection. **Save to repo** writes the project's collections to `<project>/.ferrum/api/<name>-<id>.json` (FerrumAPI JSON, all kinds; secret variable values left out); **Load** reads them back | ✅ grouping, visibility and repo export / import (rename, secrets, broken files) tested; UI not hand-tested |
| GraphQL: query editor, variables (JSON object, Beautify), operation name, POST JSON or GET (query string). **Schema** panel: introspection with the request's headers / auth, cached in SQLite per endpoint (**Reload** refetches), types with fields / args / input fields / enum values, filter. Field completion: chips under the editor list the fields of the selection set at the caret (click inserts). Pretty JSON response; cURL / Python / fetch code generation; Postman `graphql` body mode on import and export | ✅ lowering, introspection parsing, completion context, POST / GET / introspection / cache end-to-end tested; panel and chips not hand-tested |
| SOAP: SOAP 1.1 / 1.2 envelope editor, action (1.1 `SOAPAction` header, 1.2 `action` content-type parameter), default content types (a user `Content-Type` wins). **WSDL** (1.1): load from a URL, list operations per SOAP binding with action, version and address; picking one fills the action, version, URL (if empty) and a sample envelope generated from the XML Schema (elements, named / inline complex types, refs, nested up to 4 levels). Responses (and any XML body) are pretty-printed and syntax-coloured (tags, attributes, values, comments, CDATA) | ✅ envelope / headers, WSDL parsing + samples, XML pretty-printer and tokens unit-tested; WSDL load + call end-to-end tested |
| OData: service root URL + entity set / resource path and a query builder for `$filter`, `$select`, `$expand`, `$orderby`, `$top`, `$skip`, `$search`, `$count` (values percent-encoded, `$ , ( ) ' /` kept readable), live URL preview, `Accept: application/json` by default. **$metadata** (OData v2–v4 CSDL): lists entity sets (keys, properties, navigation in the tooltip); click to use one | ✅ URL building, metadata parsing, end-to-end tested |
| tRPC: base URL + procedure path, Query (GET `?input=`) or Mutation (POST JSON), batch toggle (`?batch=1`, `{"0": input}`). The response view adds a **Data** mode showing `result.data` unwrapped (a superjson-style `{json}` envelope is unwrapped too) or the `error` object | ✅ lowering and unwrapping unit-tested; query / batch / mutation / error end-to-end tested |
| WebSocket: **Connect / Disconnect** (⌘/Ctrl+Enter), ws / wss (tokio-tungstenite + rustls with the OS verifier; "Verify TLS" off accepts self-signed), handshake headers + auth, subprotocols. Composer for text, JSON (validated) and binary (base64); messages saved as templates on the request (send / edit / delete). Live log in the response panel: direction, time, frame type, size, events (connected, closed with code / reason, errors), click a row for the full (pretty) payload, Clear. The connection is recorded in history (101) | ✅ client tested against a local echo server (headers, auth, subprotocol, text / JSON / binary, server close, drop-close, refused); session log unit-tested; UI not hand-tested |
| gRPC: unary and server-streaming calls with dynamic messages (prost-reflect): schema from **server reflection** (`grpc.reflection.v1`, falling back to `v1alpha`) or a local **.proto** file compiled in pure Rust (protox) with import folders. Method picker with shape and types, message template (every field at its default), JSON message → protobuf, metadata from the Metadata table + auth, TLS (grpcs:// / https:// or the toggle; "Verify TLS" off works), timeout and cancel. Response: JSON (an array for streams), gRPC status code / name / message in the badge, headers and trailers under Metadata. Calls go through history and the collection runner. `grpcurl` command in Code | ✅ tested against a local tonic server with reflection (methods, template, metadata + Bearer, headers / trailers, stream, NOT_FOUND status, client-streaming refused, bad JSON, .proto file, refused connection, cancel) |
| **Scripts** (request sub-tab, every HTTP-based kind and gRPC): a **pre-request** and a **tests** script in a small sandboxed statement language — there is no permissively licensed JS engine in the workspace, so no JavaScript. `set env.x = …` / `global.x` / `var.x`, `unset`, `test "name": expr`, `assert expr`, `log expr`; read `status`, `time`, `size`, `body`, `json.a.b[0]` (`.length`, negative indexes), `header("Name")` / `headers["name"]`, `env.x`, and `"strings with {{vars}}"`; `== != < <= > >=`, `contains`, `matches` (regex, size-limited), `startswith`, `endswith`, `exists`, `is number/string/bool/array/object/null/empty`, `and / or / not`, `+`; `len`, `lower`, `upper`, `trim`, `number`, `string`, `uuid()`, `timestamp()`, `timestamp_ms()`, `random_int()`. Pre-request `set`s apply to the request being sent; `env.` / `global.` changes are saved (env falls back to globals with no active environment), `var.` only lives for the send / run. Lines are checked as you type. Results in the response's **Tests** tab (pass / fail with the compared values, script errors by line, variables set, `log` output). Sandboxed: no file system, no network, no loops; 64 KB script, nesting and regex size limits. The **collection runner** carries script variables to later requests and passes a request on its tests (2xx when it has none), showing `n/m tests` | ✅ language unit-tested (paths, operators, messages, errors by line, hostile input); single send and collection run end to end against a local mock server; desktop render tested; not hand-tested |
| Webhooks: the inspector is also in the sidebar header and the "New request" menu; **Save to collection…** on a capture saves it as a REST request (pointing back at the inspector) | ✅ capture → request conversion tested (existing); button not hand-tested |
| UI (GPUI): request builder with two-way URL ↔ params sync, header/param enable toggles, body/auth/settings editors, unresolved-variable highlighting, response Pretty/Raw/Preview (images inline; HTML opens in the default browser), headers/cookies tables, copy + save to file, sidebar (Collections/Environments/History) with context menus, import + code dialogs, resizable panes | ⚠️ builds, lints clean, launches; not yet clicked through |

## Not built yet
Auth gaps: Basic / Bearer / API Key values are still stored with the request (SQLite, exports), not in
the keychain; OAuth 2 implicit / password / device grants, token in the query string, and OAuth 1
RSA-SHA1 / PLAINTEXT are not supported (Postman imports note it); Digest `auth-int`; duplicated
requests / collections share one keychain entry; Hawk, NTLM, Akamai and other auth types (imported
as "Inherit" with a note). Script gaps: no JavaScript (Postman `pm.*` scripts are not imported or run),
no collection / folder-level scripts, no loops or user functions, no `setNextRequest`; load tests
and WebSocket sessions do not run scripts; variables set by a pre-request script are lost when the
request fails without a response. Runner options (iterations, delay, data
files, run report export); cookie jar gaps: cookies set on intermediate redirect responses are not captured (only the
final response's), cookies are chosen for the request URL before redirects, no public-suffix
list, session cookies persist until cleared, no per-request "don't use the jar" switch, one jar
per workspace (not per collection), and cookies are not editable in the dialog; syntax
colouring for XML/HTML bodies; regex / whole-word search; proxy and client-certificate
settings; drag-and-drop reordering / moving items between folders; keeping text per body mode when switching (switching starts the new mode empty); response
bodies over 400 KB are truncated for display (copy/save use the full body); secret *variables* in
the OS keychain (they live in the local SQLite file, never in logs); packaging and signing;
a light theme (the GPUI shell ships the dark "One Dark" base theme only; the stored
`theme` setting is kept but not applied); soft-wrap in the response body view (the shared
text field lays out unwrapped lines; wrapping needs a new layout path there).
Import gaps: Swagger 2.0 (convert to OpenAPI 3 first), external `$ref`s, OpenAPI `callbacks` /
`links` / `webhooks`, Insomnia sub-environments, template tags (`{% … %}`), gRPC / WebSocket
requests and response timelines; Bruno and Hoppscotch formats; no OpenAPI export.
Local tools gaps: mock responses are static (no templating of path params or request data into
the body, no scripts, no random/sequence variants); mocks are not copied when a collection is
duplicated and are not part of Postman export; mock servers stop when the app quits (no
auto-start); HTTP/1.1 only, no HTTPS for the mock server / inspector; webhook captures are not
persisted, cannot be exported, and binary bodies are not replayable byte-for-byte (replay uses
the lossy text); no built-in tunnel; load tests run one request (not a collection or scenario),
have no ramp-up / rate limit / think time, do not use the cookie jar, and results are not saved
or exportable.
Protocol gaps:
- GraphQL: no subscriptions (graphql-ws / SSE); completion covers fields only (no arguments,
  variables, fragments or directives) and refreshes while typing, not on caret moves; no syntax
  colouring in the query editor; cached schemas never expire (use Reload).
- SOAP: WSDL 1.1 only (no WSDL 2.0); the sample generator ignores attributes, `choice`,
  type extension and external `xsd:import`s; no WS-Security / WS-Addressing, no MTOM; the
  envelope editor is not syntax-coloured (responses are).
- OData: no `$batch`, `$apply` or `$compute`; `$filter` is free text (no expression builder);
  JSON responses only get JSON colouring (Atom XML gets XML colouring).
- tRPC: no transformers on input (superjson inputs are sent as plain JSON), no subscriptions,
  a batch holds one call; no procedure discovery (tRPC has no standard introspection).
- WebSocket: no auto-reconnect or interval pings, no cookie jar on the handshake, the log is in
  memory only (2 000 entries kept, newest 500 shown), no Socket.IO / STOMP framing; the runner
  skips WebSocket requests.
- gRPC: no client-streaming or bidi-streaming calls; server-streamed messages appear when the
  stream ends (not live); no gRPC-Web, compression or mTLS client certificates; `-bin` metadata
  is not decoded; descriptor pools are cached in memory per endpoint / .proto (not in SQLite);
  the load test is offered for HTTP-based kinds only.
- Projects: repository files are written / read on demand (Save to repo / Load), not watched or
  synced automatically; environments are global, not per project; mock-server routes are
  meaningful for REST requests only.

## Prerequisites
- Rust via rustup. `rust-toolchain.toml` installs **1.99.1** automatically.
- macOS: Xcode command line tools (GPUI renders with Metal). Linux/Windows: whatever GPUI
  needs on that platform (see Zed's Linux/Windows build docs); only macOS has been smoke-tested.
- No Node, no CSS build: styling is Rust code using the Ferrum theme.

## Commands
Run from the monorepo root (`ferrum/`).
```bash
cargo api                        # run the app (alias for: cargo run -p ferrumapi-desktop)
cargo test -p 'ferrumapi-*'      # unit + integration tests (local axum / tonic / WebSocket servers, no internet)
cargo lint                       # clippy, warnings are errors
cargo fmt --all

RUST_LOG=debug cargo api         # verbose logs (never include URLs, headers, bodies or secrets)
```

### Styling
There is no stylesheet. Components come from `ferrum-ui` (`../shared/ui`: app bootstrap,
theme, buttons, labels, icons, menus, text input, splitter, notices, modal); colours come
from the active theme (`cx.theme()`).

### Packaging
Bundling, signing and notarization are **not yet set up** for the GPUI build.

## Layout
```
apps/desktop/            GPUI app: app.rs (root view), components/{shell,request,response,sidebar,dialogs,widgets,fields}, state/, commands/
crates/domain/           pure serde types: HttpRequest, RequestBody, Auth, HttpResponse, Collection tree ops,
                         Environment, VariableSet ({{var}} scan/resolve), URL ↔ params sync, history, workspace,
                         CookieJar (RFC 6265 storage/matching), highlight (find matches, JSON tokens, span layering),
                         mock (route paths + matching), webhook (captures, replay), load (caps, percentiles, histogram)
                         protocol (request kinds: GraphQL / SOAP / OData / tRPC lowering, GraphQL schema + completion,
                         WebSocket log, gRPC method info), xml (pretty-print, tokens)
crates/server/           hyper HTTP/1.1 servers bound to 127.0.0.1: mock server, webhook inspector
crates/http/             reqwest + rustls execution: substitution, auth, bodies, cookies, cancellation, error mapping;
                         WebSocket client (tokio-tungstenite), shared rustls config
crates/grpc/             gRPC with dynamic messages: reflection client, .proto compilation (protox), unary / server streaming (tonic)
crates/persistence/      SQLite app state + migrations (collections + project, environments, globals, history, tabs, settings,
                         cookies, mocks, schema cache)
crates/interop/          Postman v2.1 import/export, Postman environments, cURL import, code generation, WSDL, OData $metadata
crates/application/      AppServices runtime, prepare (auth inheritance + variable layers), send + history, cookie jar,
                         collection runner, mock route planning, load-test runner, file transfer, protocol services
                         (introspection, WSDL, $metadata, gRPC, WebSocket), project repository files
docs/                    ARCHITECTURE
```

## Local data
`~/Library/Application Support/dev.FerrumAPI.FerrumAPI/ferrumapi.db` on macOS (OS-appropriate
elsewhere via `directories`). Deleting it resets FerrumAPI. Response bodies are never stored;
cookies from the cookie jar are (clear them from the **Cookies** dialog).
