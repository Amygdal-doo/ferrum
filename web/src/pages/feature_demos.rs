//! Animated mock windows for the features page, one per app.
//!
//! Plain HTML animated by CSS (`assets/css/features.css`, `.demo*` classes),
//! no images or JavaScript. Each loops every 9s; with reduced motion the
//! global rule in base.css stops them and the static layout still reads as a
//! finished screen. The caller wraps each demo in `role="img"` with a label.

use maud::{Markup, html};

use super::project_demos;
use crate::products;

/// The demo for a product, by slug.
pub fn for_slug(slug: &str) -> Option<Markup> {
    let demo: fn() -> Markup = match slug {
        s if s == products::FERRUMAPI.slug => api,
        s if s == products::FERRUMGIT.slug => git,
        s if s == products::FERRUMDATA.slug => data,
        s if s == products::FERRUMTERM.slug => term,
        s if s == products::FERRUMKIT.slug => kit,
        s if s == products::FERRUMDOCK.slug => dock,
        s if s == products::FERRUMPORTS.slug => project_demos::ports,
        s if s == products::FERRUMENV.slug => project_demos::env,
        s if s == products::FERRUM_EDITOR.slug => editor,
        s if s == products::FERRUM_LAUNCHER.slug => launcher,
        _ => return None,
    };
    Some(demo())
}

/// Window chrome: traffic lights, a title and the app-specific body.
pub(super) fn window(title: &str, body_class: &str, body: Markup) -> Markup {
    html! {
        div class="demo" {
            div class="demo__bar" {
                span class="demo__dots" { i {} i {} i {} }
                span class="demo__title" { (title) }
            }
            div class={ "demo__body " (body_class) } { (body) }
        }
    }
}

pub fn api() -> Markup {
    window(
        "FerrumAPI — Users",
        "api",
        html! {
            div class="api__req" {
                span class="api__method" { "GET" }
                span class="api__url" { "{{base}}/users?limit=2" }
                span class="api__send" { "Send" }
            }
            div class="api__panes" {
                div class="api__resp" {
                    div class="api__find" { "⌘F " span { "alice" } em { "1 of 2" } }
                    p { "[" }
                    p { "  { " span class="t-key" { "\"id\"" } ": " span class="t-num" { "1" } "," }
                    p { "    " span class="t-key" { "\"name\"" } ": " span class="t-str" { "\"" mark class="api__mark" { "Alice" } "\"" } "," }
                    p { "    " span class="t-key" { "\"admin\"" } ": " span class="t-kw" { "true" } " }," }
                    p { "  { " span class="t-key" { "\"id\"" } ": " span class="t-num" { "2" } "," }
                    p { "    " span class="t-key" { "\"invited_by\"" } ": " span class="t-str" { "\"" mark class="api__mark" { "alice" } "\"" } " }" }
                    p { "]" }
                }
                div class="api__runner" {
                    p class="demo__label" { "Runner · Users" }
                    ul {
                        li { span class="demo__check" { "✓" } "List users" em { "200" } }
                        li { span class="demo__check" { "✓" } "Create user" em { "201" } }
                        li { span class="demo__check" { "✓" } "Get user" em { "200" } }
                        li { span class="demo__check" { "✓" } "Delete user" em { "204" } }
                    }
                    p class="api__cookies" { "Cookies (2) · session, theme" }
                }
            }
        },
    )
}

pub fn git() -> Markup {
    let commits = [
        ("3f2a91c", "Fix lexer on CRLF input", true),
        ("9b04e2d", "Merge branch 'parser'", false),
        ("51cd7a0", "Parser: recover after errors", false),
        ("e7a3b19", "Fix panic in parser tests", true),
        ("0c88f41", "Initial lexer", false),
    ];
    window(
        "FerrumGIT — ferrum",
        "git",
        html! {
            div class="git__log" {
                div class="git__search" { "⌕ fix" em { "1 of 2" } }
                ul {
                    @for (i, (hash, msg, hit)) in commits.iter().enumerate() {
                        li class={ "git__commit" @if i == 1 || i == 2 { " lane-2" } @if *hit { " is-hit" } } {
                            span class="git__node" {}
                            code { (hash) }
                            span { (msg) }
                        }
                    }
                }
            }
            div class="git__diff" {
                p class="demo__label" { "src/lexer.rs · Side by side" }
                div class="git__cols" {
                    div {
                        p { "12  let line = src;" }
                        p class="git__del" { "13  line.split('\\n')" }
                        p { "14  .map(trim)" }
                        p class="git__del" { "15  .collect()" }
                    }
                    div {
                        p { "12  let line = src;" }
                        p class="git__add" { "13  line.lines()" }
                        p { "14  .map(trim)" }
                        p class="git__add" { "15  .collect::<Vec<_>>()" }
                    }
                }
                p class="git__blame" { "Blame · e7a3b19 · 2 days ago" }
            }
            div class="demo__toast" { "Committed 3f2a91c" span { "Undo" } }
        },
    )
}

pub fn data() -> Markup {
    let rows = [
        ("1", "Alice", "pro"),
        ("4", "Dmitri", "team"),
        ("2", "Jun", "free"),
        ("3", "Maya", "pro"),
    ];
    window(
        "FerrumData — prod-readonly",
        "data",
        html! {
            div class="data__side" {
                p class="demo__label" { "History" }
                ul {
                    li class="is-new" { "SELECT id, name…" }
                    li { "UPDATE plans…" }
                    li { "SELECT count(*)…" }
                }
                p class="demo__label" { "users" }
                ul class="data__cols" {
                    li { "id " em { "int8 · PK" } }
                    li { "name " em { "text" } }
                    li { "plan " em { "text" } }
                }
            }
            div class="data__main" {
                p class="data__query" {
                    span class="data__typed" { "SELECT id, name, plan FROM users ORDER BY name;" }
                    span class="demo__caret" {}
                }
                div class="data__grid" {
                    div class="data__row data__head" {
                        span { "id" } span { "name " i class="data__sort" { "▲" } } span { "plan" }
                    }
                    @for (id, name, plan) in rows {
                        div class="data__row" { span { (id) } span { (name) } span { (plan) } }
                    }
                }
                ul class="data__menu" {
                    li { "Export CSV…" }
                    li { "Export JSON…" }
                    li { "Copy as Markdown" }
                }
            }
        },
    )
}

pub fn term() -> Markup {
    window(
        "FerrumTerm — work",
        "term",
        html! {
            div class="term__side" {
                p class="demo__label" { "Sessions" }
                ul {
                    li class="is-open" { "work" em { "3 tabs" } }
                    li { "web" em { "1 tab" } }
                }
                p class="demo__label" { "Agents" }
                ul {
                    li {
                        i class="term__dot term__dot--run" {}
                        "Claude Code"
                        em class="term__state" { span { "working" } span { "done" } }
                    }
                    li {
                        i class="term__dot term__dot--wait" {}
                        "Codex"
                        b class="term__badge" { "1" }
                    }
                }
                p class="term__new" { "+ New agent" }
            }
            div class="term__pane" {
                p { span class="t-prompt" { "~/ferrum ❯ " } "cargo test --workspace" }
                p class="term__out" { "   Compiling ferrum-core v0.5.0" }
                p class="term__out" { "running 531 tests" }
                p class="term__out" { span class="term__select" { "test result: ok. 531 passed; 0 failed" } }
                p class="term__out" { span class="t-prompt" { "~/ferrum ❯ " } span class="demo__caret" {} }
                span class="demo__toast term__copied" { "Copied" }
            }
        },
    )
}

pub fn launcher() -> Markup {
    let apps = [
        ("Ferrum Editor", "--app-editor", "0.1.0-alpha.1"),
        ("FerrumAPI", "--app-api", "0.2.0-alpha.1"),
        ("FerrumGIT", "--app-git", "0.2.0-alpha.1"),
        ("FerrumData", "--app-data", "0.3.0-alpha.1"),
        ("FerrumTerm", "--app-term", "0.2.0-alpha.1"),
    ];
    window(
        "Ferrum Launcher",
        "tb",
        html! {
            div class="tb__banner" {
                span { "Ferrum Launcher 0.5.0 is available" }
                span class="tb__btn tb__btn--self" { "Update Launcher" }
            }
            ul class="tb__apps" {
                @for (name, var, version) in apps {
                    li style={ "--c: var(" (var) ")" } {
                        i class="tb__logo" {}
                        span class="tb__name" { (name) }
                        span class="tb__chan" { "Alpha" }
                        span class="tb__ver" { "v" (version) }
                        span class="tb__btn" { span { "Update" } span { "✓ Up to date" } }
                    }
                }
            }
        },
    )
}

pub fn kit() -> Markup {
    let tools = [
        "JSON format",
        "JWT decode",
        "Base64",
        "URL parse",
        "Timestamp",
        "UUID / ULID",
        "Hash",
        "Regex",
    ];
    window(
        "FerrumKit",
        "kit",
        html! {
            div class="kit__side" {
                p class="kit__search" { "⌘K " span { "jwt" } }
                ul {
                    @for (i, tool) in tools.iter().enumerate() {
                        li class=[(i == 1).then_some("is-open")] { (tool) }
                    }
                }
            }
            div class="kit__main" {
                p class="demo__label" { "Token" }
                p class="kit__token" {
                    span class="t-kw" { "eyJhbGciOiJIUzI1NiJ9" } "."
                    span class="t-num" { "eyJzdWIiOiJhbGljZSIsImV4cCI6MTc2Mjc4MDAwMH0" } "."
                    span class="t-str" { "kM3p0Qe…" }
                }
                div class="kit__out" {
                    div {
                        p class="demo__label" { "Header" }
                        p { "{ " span class="t-key" { "\"alg\"" } ": " span class="t-str" { "\"HS256\"" } " }" }
                    }
                    div {
                        p class="demo__label" { "Payload" }
                        p { "{ " span class="t-key" { "\"sub\"" } ": " span class="t-str" { "\"alice\"" } "," }
                        p { "  " span class="t-key" { "\"exp\"" } ": " span class="t-num" { "1762780000" } " }" }
                        p class="kit__exp" { "exp 10 Nov 2025, 13:06 " b { "Expired" } }
                    }
                }
                span class="demo__toast" { "Payload copied" }
            }
        },
    )
}

pub fn dock() -> Markup {
    let containers = [
        ("api", "shop-api:dev", "8080→80", 62),
        ("db", "postgres:17", "5432", 24),
        ("cache", "redis:7", "6379", 8),
    ];
    window(
        "FerrumDock — orbstack",
        "dock",
        html! {
            ul class="dock__side" {
                li class="is-open" { "Containers" em { "5" } }
                li { "Images" em { "12" } }
                li { "Volumes" em { "4" } }
                li { "Networks" em { "3" } }
            }
            div class="dock__main" {
                p class="dock__group" { "▾ shop " em { "Compose · 3 running" } span { "Restart" } }
                @for (name, image, ports, cpu) in containers {
                    div class="dock__row" style={ "--cpu: " (cpu) "%" } {
                        i class="dock__dot" {}
                        span class="dock__name" { (name) }
                        span class="dock__image" { (image) }
                        span class="dock__ports" { (ports) }
                        span class="dock__cpu" { b {} }
                    }
                }
                p class="dock__group dock__group--off" { "▸ blog " em { "Compose · stopped" } }
                div class="dock__logs" {
                    p class="demo__label" { "api · logs" }
                    p { span class="t-prompt" { "INFO" } " listening on 0.0.0.0:80" }
                    p { span class="t-prompt" { "INFO" } " GET /health 200 2ms" }
                    p { span class="t-kw" { "WARN" } " slow query 812ms" }
                }
                span class="demo__toast" { "Restarted shop" span { "Logs" } }
            }
        },
    )
}

pub fn editor() -> Markup {
    // Three carets type `pub ` at once (multi-cursor), then the command
    // palette opens on "toggle".
    let fields = [
        ("bg", "0x14110d"),
        ("fg", "0xf1e9df"),
        ("accent", "0xc5dc3a"),
    ];
    window(
        "theme.rs — notes",
        "ed",
        html! {
            ul class="ed__tree" {
                li { "▾ notes" }
                li class="ed__in" { "▾ src" }
                li class="ed__in2" { "main.rs" }
                li class="ed__in2 is-open" { "theme.rs" }
                li class="ed__in" { "▸ snippets" }
                li class="ed__in" { "keymap.toml" }
                li class="ed__in" { "README.md" }
            }
            div class="ed__main" {
                ul class="ed__tabs" {
                    li class="is-active" { "theme.rs" i class="ed__dirty" {} }
                    li { "keymap.toml" }
                    li { "README.md" }
                }
                div class="ed__code" {
                    p { span class="t-kw" { "pub struct " } span class="t-ty" { "Theme" } " {" }
                    @for (name, _) in fields {
                        p class="ed__multi" {
                            "    "
                            span class="ed__typed" { span class="t-kw" { "pub " } }
                            i class="ed__caret" {}
                            (name) ": " span class="t-ty" { "Rgb" } ","
                        }
                    }
                    p { "}" }
                    p { " " }
                    p { span class="t-kw" { "pub fn " } span class="t-fn" { "dark" } "() -> " span class="t-ty" { "Theme" } " {" }
                    @for (name, hex) in fields {
                        p { "    " (name) ": " span class="t-fn" { "rgb" } "(" span class="t-num" { (hex) } ")," }
                    }
                    p { "}" }
                }
                div class="ed__palette" {
                    p class="ed__query" { "› toggle" span class="demo__caret" {} }
                    ul {
                        li class="ed__sel" {}
                        li { span { "Editor: " b { "Toggle" } " Comment" } kbd { "⌘/" } }
                        li { span { "Editor: " b { "Toggle" } " Soft Wrap" } kbd { "⌥Z" } }
                        li { span { "Tree View: " b { "Toggle" } } kbd { "⌘\\" } }
                        li { span { "Markdown Preview: " b { "Toggle" } } kbd { "⇧⌘M" } }
                    }
                }
            }
            div class="ed__status" {
                span { "src/theme.rs" }
                span class="ed__cursors" { "3 cursors" }
                span class="ed__meta" { "Ln 2, Col 9 · Rust · LF · ⎇ main" }
            }
        },
    )
}
