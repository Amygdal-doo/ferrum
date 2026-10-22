# web-ferrum

Server-rendered marketing site for the **Ferrum** family of Rust developer tools
(FerrumData, FerrumAPI, Ferrum IDE). It is a single Rust binary built with
Axum 0.8, maud templates and tower-http. All CSS, JS and images are embedded in
the binary with rust-embed, so there is nothing to upload besides the binary.

> **Status: not deployed yet.** The Railway configuration below has been
> tested locally with Docker only. Nothing has been pushed to Railway.

## Local development

Requires Rust 1.99.1. `rust-toolchain.toml` pins it, and rustup installs it automatically.

```bash
cargo run                      # http://localhost:8080
PORT=3000 cargo run            # another port
RUST_LOG=debug cargo run       # more verbose logs
cargo test                     # unit + HTTP integration tests
cargo clippy --all-targets -- -D warnings
```

In debug builds rust-embed reads `assets/` from disk, so you can edit CSS or JS
and reload without recompiling. Release builds embed the files.

## Environment variables

| Variable                | Default                  | Purpose |
|-------------------------|--------------------------|---------|
| `PORT`                  | `8080`                   | Listen port (bound on `0.0.0.0`). Railway sets it automatically. |
| `SITE_URL`              | see below                | Public base URL used for canonical, Open Graph, robots and sitemap URLs, e.g. `https://ferrum.dev`. A trailing `/` is ignored. |
| `RAILWAY_PUBLIC_DOMAIN` | (set by Railway)         | Fallback when `SITE_URL` is unset: becomes `https://$RAILWAY_PUBLIC_DOMAIN`. |
| `RUST_LOG`              | `info`                   | `tracing` env filter, e.g. `info,tower_http=debug`. |

If neither `SITE_URL` nor `RAILWAY_PUBLIC_DOMAIN` is set, the site URL is
`http://localhost:$PORT`.

## Routes

| Path | Notes |
|------|-------|
| `/`, `/ferrumdata`, `/ferrumapi`, `/ferrum-ide`, `/about` | HTML pages, `Cache-Control: public, max-age=300` |
| `/healthz` | `200 ok`, `text/plain`, `no-store`, used by the Railway health check |
| `/robots.txt`, `/sitemap.xml` | The sitemap is built from `products::ALL` plus `/` and `/about` |
| `/assets/*` | Embedded files. URLs from `assets::url()` carry `?v=<sha256 prefix>` and are served `immutable` for one year. Every asset has a strong ETag (`304` on `If-None-Match`) |
| anything else | HTML 404, `no-cache` |

Every response gets gzip/brotli compression when the client accepts it, plus
`X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`
and `X-Frame-Options: DENY`. No CSP is set yet. Any CSP added later must allow
Google Fonts (`fonts.googleapis.com` and `fonts.gstatic.com`).

## Project layout

```
src/
  main.rs          tracing setup, bind, graceful shutdown (SIGTERM / Ctrl+C)
  lib.rs           crate root, re-exports `app` and `Config`
  config.rs        env parsing (pure `Config::from_vars`, unit-tested)
  site.rs          site-wide constants and ALL placeholder links
  products.rs      product catalogue (drives nav, cards, sitemap)
  layout.rs        HTML shell (head, meta, nav, footer)
  components/      reusable maud components (nav, footer, card, badge, ...)
  pages/           one module per page, each with `render(site_url)`
  routes/          router (mod.rs), assets, health, seo, middleware
assets/            css/, js/, img/ (embedded at compile time)
tests/http.rs      end-to-end HTTP tests via tower::ServiceExt::oneshot
Dockerfile         cargo-chef multi-stage build -> debian:bookworm-slim, uid 10001
railway.toml       Railway build + deploy config
```

### Placeholders

All not-yet-real values live in **`src/site.rs`**: the GitHub org and repo URLs
(`https://github.com/your-org/...`), the contact email (`None`) and the download
label ("Coming soon"). Replace them there before launch. Pages must not hard-code
links.

## Docker

```bash
docker build -t web-ferrum .
docker run --rm -p 8080:8080 -e SITE_URL=http://localhost:8080 web-ferrum
curl localhost:8080/healthz   # ok
```

The builder uses `rust:1.99.1-slim-bookworm`, which matches `rust-toolchain.toml`.
When you bump the toolchain, update `ARG RUST_VERSION` in the `Dockerfile` as well.
The runtime image runs as a non-root user (uid 10001), and the binary is at
`/usr/local/bin/web-ferrum`.

## Deploying to Railway (not done yet)

`railway.toml` tells Railway to build with the `Dockerfile` and to health-check
`/healthz`, with a 100 s timeout and restart-on-failure (up to 10 retries).

1. Create a project from the GitHub repo: Railway dashboard → *New Project* →
   *Deploy from GitHub repo*. Alternatively, use the CLI from this directory:
   ```bash
   railway login
   railway init        # or: railway link
   railway up
   ```
2. In the service's *Settings → Networking*, click **Generate Domain**, or
   attach a custom domain.
3. In *Variables*, set `SITE_URL` to the public URL, e.g.
   `https://web-ferrum-production.up.railway.app` or your custom domain.
   Without it, the app falls back to `RAILWAY_PUBLIC_DOMAIN`. Do not set `PORT`.
   Railway injects it.
4. Redeploy, then check `https://<domain>/healthz` and `https://<domain>/sitemap.xml`.

On redeploy Railway sends SIGTERM, and the server drains in-flight connections
before it exits.
