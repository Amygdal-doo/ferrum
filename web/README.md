# web-ferrum

Server-rendered marketing site for the **Ferrum** family of open-source Rust developer tools
(FerrumIDE, FerrumEngineer, FerrumAPI, FerrumGIT, FerrumData, FerrumTerm, FerrumKit, FerrumContainers,
FerrumPorts, FerrumVariables and FerrumLauncher). Project logs and GitHub Actions live
in FerrumIDE's Logs and CI pages, presented as sections of `/ferrumide`.
It is a single Rust binary built with
Axum 0.8, maud templates and tower-http. All CSS, JS and images are embedded in
the binary with rust-embed, so there is nothing to upload besides the binary.

> **Deployed on Railway** at <https://tryferrum.dev>
> (`SITE_URL` is set there).

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

There is no mailing list: news goes on `/updates` and its Atom feed.

## Routes

| Path | Notes |
|------|-------|
| `/`, `/features`, `/ferrumide`, `/ferrumapi`, `/ferrumgit`, `/ferrumdata`, `/ferrumterm`, `/ferrumkit`, `/ferrumcontainers`, `/ferrumports`, `/ferrumvariables`, `/ferrumengineer`, `/ferrum-launcher`, `/download`, `/about` | HTML pages, `Cache-Control: public, max-age=300` |
| `/docs`, `/docs/install`, `/docs/shell-commands`, `/docs/project-bar`, `/docs/shortcuts` | Guides (`src/pages/docs/`), with a shared index rail. Shortcuts and shell commands are taken from the apps' code (`shell.rs`, each app's key bindings) |
| `/changelog` | Every release of every app, newest first, from the same cached GitHub releases list as `/download/macos`. If GitHub fails, the last good list is still served. If GitHub has never answered, the page says so and links the releases page |
| `/faq`, `/security` | FAQ (with `FAQPage` JSON-LD) and the security model (signing, checksums, the FerrumVariables vault and its limits) |
| `/compare`, `/compare/{slug}` | `ferrumcontainers-vs-docker-desktop`, `ferrumapi-vs-postman`. Ferrum's column comes from the READMEs. The other column uses general terms only, with a note that its details can change |
| `/ferrumdock` | `308` to `/ferrumcontainers` (FerrumDock was renamed) |
| `/ferrum-editor`, `/ferrumvibe`, `/ferrumagentic`, `/ferrumenv` | `308` to `/ferrumide`, `/ferrumengineer` (twice renamed: FerrumAgentic, FerrumVibe) and `/ferrumvariables` (Ferrum Editor, FerrumVibe and FerrumEnv were renamed on 2026-11-15) |
| `/favicon.ico` | `308` to the 32px PNG favicon |
| `/ferrum-toolbox`, `/ferrum-ide`, `/ferrumlogs`, `/ferrumci` | `308` to `/ferrum-launcher` (the launcher's former name), `/ferrumide` (the old Ferrum IDE app was dropped; FerrumIDE, formerly Ferrum Editor, is the suite's code editor), `/ferrumide#logs` and `/ferrumide#ci` (FerrumLogs and FerrumCI became the editor's Logs and CI pages) |
| `/download/macos` | `302` to the newest stable `launcher-v*` (or older `toolbox-v*`) `*-aarch64.dmg` in the public `Amygdal-doo/ferrum-releases` repo. The release list comes from the GitHub API (anonymous) and is cached in memory for 10 min (`releases::Releases`, shared with `/changelog`). After a failed fetch, GitHub is asked again after 1 min. Without a list, it links the releases page. `no-store`. Disallowed in robots.txt |
| `/updates`, `/updates/{slug}`, `/privacy` | HTML pages. Posts are Rust data in `src/updates.rs` with bodies in `src/pages/posts/`. The updates pages end with a block at `#follow` that points to the Atom feed |
| `/updates/feed.xml` | Atom feed of every post, full HTML content with absolute links. Linked from every page's `<head>` |
| `/healthz` | `200 ok`, `text/plain`, `no-store`, used by the Railway health check |
| `/robots.txt`, `/sitemap.xml` | The sitemap lists every page with a `lastmod`: `site::CONTENT_UPDATED` for static copy, post dates for posts, and the newest cached release for `/changelog` |
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
  site.rs          site-wide constants and links (download, releases, upstream credits)
  products.rs      product catalogue (drives nav, cards, sitemap)
  releases.rs      cached GitHub releases list: launcher DMG for /download/macos and /changelog entries
  seo.rs           JSON-LD builders (Organization, WebSite, BreadcrumbList, SoftwareApplication, FAQPage, BlogPosting)
  updates.rs       posts on /updates (slug, title, date, byline, summary, body fn)
  layout.rs        HTML shell: head with title, description, canonical, Open Graph, Twitter,
                   theme-color, icons, JSON-LD; nav and footer
  components/      the design system's maud components (see DESIGN.md)
  pages/           one module per page, each with `render(site_url)`; posts/ holds post bodies
  routes/          router (mod.rs), assets, health, seo, middleware, updates
assets/            css/, js/, img/ (embedded at compile time)
tests/http.rs      end-to-end HTTP tests via tower::ServiceExt::oneshot (pages, redirects, feed,
                   SEO metadata and parsed JSON-LD, sitemap). Releases come from fake fetchers:
                   no test touches the network
tools/og/          social image template + render.sh (headless Chrome) -> assets/img/og/<slug>.png
DESIGN.md          the design system: tokens, components, rules
Dockerfile         cargo-chef multi-stage build -> debian:bookworm-slim, uid 10001
railway.toml       Railway build + deploy config
```

### Links and brand assets

Site-wide links live in **`src/site.rs`**, including the source repository
(`Amygdal-doo/ferrum`), its issues, discussions, CONTRIBUTING and security-advisory
links, the licence (`MIT OR Apache-2.0`) and the three build-from-source commands: every download CTA points to
`/download`, whose primary button is `/download/macos`. Builds are public
GitHub Releases in `Amygdal-doo/ferrum-releases` (no sign-in needed). Set
`site::MACOS_NOTARIZED = true` once notarized builds ship to hide the
Gatekeeper "Open Anyway" section. The contact email is still `None`. Pages must not hard-code
links.

Logos in `assets/img/brand/` are copies of `../brand/svg/` (the Docker build
context is `web/`, so they must live here). After regenerating the brand,
copy them again. `assets/img/og.png` (1200×630, the default social image) is rendered from
`assets/img/og.svg`, e.g. `sips -s format png assets/img/og.svg --out assets/img/og.png`.
Each app has its own social image, `assets/img/og/<slug>.png`. To render them all
again, run `tools/og/render.sh`, which needs Google Chrome. Keep its rows in sync with
`src/products.rs`; a unit test checks that every product has one.

## SEO

- Every page has a unique `<title>` and meta description (the HTTP tests check
  uniqueness and length), a canonical URL from `SITE_URL`, Open Graph and Twitter cards,
  and exactly one `<h1>`.
- JSON-LD: `Organization` (AMYGDAL d.o.o., Sarajevo) and `WebSite` on the home page.
  `BreadcrumbList` on every page with a trail. `SoftwareApplication` on app pages (macOS and
  Linux, `DeveloperApplication`, `price: 0`, because the site says every build is free,
  `license` MIT and Apache-2.0, and `isBasedOn` a `SoftwareSourceCode` with the
  `codeRepository`).
  `FAQPage` on `/faq` and `BlogPosting` on posts.
- The 404 page is `noindex` and has no canonical URL.
- `<html lang="en">`, light and dark `theme-color`, an SVG and a 32px PNG favicon, the
  apple-touch-icon, `site.webmanifest`.

## Design system

See [DESIGN.md](DESIGN.md).

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

## Deploying to Railway

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
   `https://tryferrum.dev` or your custom domain.
   Without it, the app falls back to `RAILWAY_PUBLIC_DOMAIN`. Do not set `PORT`.
   Railway injects it.
4. Redeploy, then check `https://<domain>/healthz` and `https://<domain>/sitemap.xml`.

On redeploy Railway sends SIGTERM, and the server drains in-flight connections
before it exits.
