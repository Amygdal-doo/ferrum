//! Animated mock windows for the per-project apps: FerrumPorts, FerrumEnv,
//! and the Logs and CI pages of Ferrum Editor (shown on the editor's page at
//! `#logs` and `#ci`). Same rules as `feature_demos` (CSS-only, 9s
//! loop, a still frame under reduced motion); each window opens with the
//! shared Ferrum project strip. Styles: `assets/css/features.css`.

use maud::{Markup, html};

use super::feature_demos::window;

/// The project strip every one of these apps has on top: name, path, branch,
/// and buttons to open the project in Editor, Git or Term.
fn project_strip(name: &str, path: &str, branch: &str) -> Markup {
    html! {
        div class="pstrip" {
            span class="pstrip__name" { (name) " ▾" }
            span class="pstrip__path" { (path) }
            span class="pstrip__branch" { "⎇ " (branch) }
            span class="pstrip__open" { span { "Editor" } span { "Git" } span { "Term" } }
        }
    }
}

pub fn ports() -> Markup {
    let shop = [
        ("3000", "node", "pnpm dev", "web"),
        ("5432", "postgres", "postgres -D /data", "Postgres"),
        ("9229", "node", "node --inspect", "Node inspector"),
    ];
    window(
        "FerrumPorts",
        "ports",
        html! {
            (project_strip("shop", "~/code/shop", "main"))
            div class="ports__tools" {
                span class="ports__search" { "⌕ " span { "30" } }
                span class="ports__toggle" { "UDP" }
                span class="ports__check" { "Port " b { "3001" } " · free" }
                em { "Auto-refresh 2s" }
            }
            div class="ports__table" {
                p class="ports__group" { "shop " em { "3000 web, 5432 postgres, 9229 inspector" } }
                @for (i, (port, process, command, hint)) in shop.iter().enumerate() {
                    div class={ "ports__row" @if i == 0 { " is-picked" } } {
                        b { (port) }
                        span { (process) }
                        span class="ports__cmd" { (command) }
                        em { (hint) }
                    }
                }
                p class="ports__group" { "Other" }
                div class="ports__row" {
                    b { "6379" } span { "redis-server" } span class="ports__cmd" { "redis-server *:6379" } em { "Redis" }
                }
            }
            div class="ports__dialog" {
                p { b { "Stop node (pid 48213)?" } }
                p class="ports__cmd" { "pnpm dev · ~/code/shop" }
                p class="ports__actions" { span { "Cancel" } span class="ports__stop" { "Send SIGTERM" } }
            }
            span class="demo__toast" { "Stopped node · 3000 is free" }
        },
    )
}

pub fn env() -> Markup {
    let vars = [
        ("DATABASE_URL", "postgres://shop@localhost/shop", true),
        ("STRIPE_KEY", "sk_test_51Hx…", true),
        ("PORT", "3000", false),
        ("LOG_LEVEL", "debug", false),
    ];
    window(
        "FerrumEnv",
        "env",
        html! {
            (project_strip("shop", "~/code/shop", "main"))
            div class="env__body" {
                ul class="env__side" {
                    li class="is-open" { "shop" em { "3" } }
                    li { "blog" em { "2" } }
                    li { "infra" em { "4" } }
                }
                div class="env__main" {
                    ul class="env__tabs" {
                        li class="is-active" { "development" }
                        li { "staging" }
                        li { "production" }
                    }
                    @for (i, (key, value, secret)) in vars.iter().enumerate() {
                        div class={ "env__row" @if i == 0 { " is-revealed" } } {
                            span class="env__key" { (key) }
                            @if *secret {
                                span class="env__val" {
                                    span class="env__mask" { "••••••••••••" }
                                    span class="env__plain" { (value) }
                                }
                                i class="env__lock" { "secret" }
                            } @else {
                                span class="env__val" { (value) }
                            }
                        }
                    }
                    p class="env__status" { "shop · development · Locks in 29 min" }
                }
            }
            span class="demo__toast" { "Copied · clears in 30 s" }
        },
    )
}

/// Ferrum Editor's Logs page: a project's sources merged into one live view.
pub fn logs() -> Markup {
    let lines = [
        ("web", "12:04:01", "INFO", "ready on http://localhost:3000"),
        ("db", "12:04:02", "INFO", "database system is ready"),
        ("web", "12:04:07", "INFO", "GET /cart 200 18ms"),
        ("web", "12:04:09", "ERROR", "TypeError: price is undefined"),
        ("app", "12:04:09", "WARN", "retrying payment webhook (2/3)"),
    ];
    window(
        "Ferrum Editor · Logs",
        "logs",
        html! {
            (project_strip("shop", "~/code/shop", "main"))
            div class="logs__body" {
                ul class="logs__side" {
                    li { i class="logs__dot" {} "web" em { "pnpm dev" } }
                    li { i class="logs__dot" {} "db" em { "compose" } }
                    li { i class="logs__dot logs__dot--file" {} "app" em { "logs/*.log" } }
                    li class="logs__suggest" { "+ cargo run" }
                }
                div class="logs__main" {
                    div class="logs__tools" {
                        span class="logs__lvl is-on" { "Error" }
                        span class="logs__lvl is-on" { "Warn" }
                        span class="logs__lvl is-on" { "Info" }
                        span class="logs__search" { "⌕ " span { "price" } }
                    }
                    @for (source, time, level, text) in lines {
                        p class={ "logs__line logs__line--" (level.to_lowercase()) } {
                            span class="logs__chip" { (source) }
                            em { (time) }
                            b { (level) }
                            span { (text) }
                        }
                    }
                    span class="logs__jump" { "Jump to latest (12 new)" }
                }
            }
        },
    )
}

/// Ferrum Editor's CI page: GitHub Actions runs, a job's steps and its log.
pub fn ci() -> Markup {
    let runs = [
        ("is-run", "●", "Fix cart total rounding", "CI #212 · main"),
        ("is-ok", "✓", "Add webhook retries", "CI #211 · main"),
        ("is-fail", "✗", "Bump serde", "CI #210 · deps"),
        ("is-ok", "✓", "Release 0.4.0", "Deploy #88 · main"),
    ];
    window(
        "Ferrum Editor · CI",
        "ci",
        html! {
            (project_strip("shop", "~/code/shop", "main"))
            div class="ci__body" {
                ul class="ci__runs" {
                    @for (state, icon, title, meta) in runs {
                        li class=(state) {
                            i { (icon) }
                            span { (title) em { (meta) } }
                        }
                    }
                }
                div class="ci__detail" {
                    p class="demo__label" { "CI #210 · test" }
                    ul class="ci__steps" {
                        li class="is-ok" { "✓ Checkout" em { "2s" } }
                        li class="is-ok" { "✓ Build" em { "1m 12s" } }
                        li class="is-fail" { "✗ cargo test" em { "38s" } }
                    }
                    div class="ci__log" {
                        p { "▸ Run cargo test --workspace" }
                        p class="ci__err" { "error[E0308]: mismatched types" }
                        p { "  --> src/cart.rs:42:9" }
                    }
                    p class="ci__actions" { span { "Re-run failed jobs" } span { "Open on GitHub" } }
                }
            }
            span class="demo__toast" { "CI #212 passed" span { "Show" } }
        },
    )
}
