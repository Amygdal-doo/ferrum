//! Animated mock windows for the features page, one per app.
//!
//! Plain HTML animated by CSS (`assets/css/features.css`, `.demo*` classes),
//! no images or JavaScript. Each loops every 9s; with reduced motion the
//! global rule in base.css stops them and the static layout still reads as a
//! finished screen. The caller wraps each demo in `role="img"` with a label.

use maud::{Markup, html};

/// Window chrome: traffic lights, a title and the app-specific body.
fn window(title: &str, body_class: &str, body: Markup) -> Markup {
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

pub fn ide() -> Markup {
    window(
        "lexer.rs — ferrum",
        "ide",
        html! {
            ul class="ide__tree" {
                li { "▾ src" }
                li class="is-open" { "lexer.rs" }
                li { "parser.rs" }
                li { "main.rs" }
                li { "Cargo.toml" }
            }
            div class="ide__editor" {
                p { span class="t-kw" { "pub fn " } span class="t-fn" { "tokens" } "(src: &" span class="t-kw" { "str" } ") -> " span class="t-ty" { "Vec" } "<Token> {" }
                p { "    " span class="t-kw" { "let mut " } "out = " span class="t-ty" { "Vec" } "::new();" }
                p { "    " span class="t-kw" { "for " } "ch " span class="t-kw" { "in " } "src.chars() {" }
                p { "        out.push(" span class="t-ty" { "Token" } "::from(ch));" }
                p { "    }" }
                p { span class="ide__typed" { "    out" } span class="demo__caret" {} }
                p { "}" }
            }
            div class="ide__palette" {
                p class="ide__query" { "› ferrum: open" }
                ul {
                    li class="ide__sel" {}
                    li { "Open in FerrumGIT" kbd { "⌃⌥⌘G" } }
                    li { "Open FerrumTerm here" kbd { "⌃⌥⌘T" } }
                    li { "Open FerrumData" }
                    li { "Open FerrumAPI" }
                }
            }
            div class="ide__status" {
                span { "⎇ main" }
                span class="ide__ferrum" { "◆ Ferrum" }
            }
        },
    )
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

pub fn toolbox() -> Markup {
    let apps = [
        ("Ferrum IDE", "--app-ide", "0.3.0-alpha.1"),
        ("FerrumAPI", "--app-api", "0.2.0-alpha.1"),
        ("FerrumGIT", "--app-git", "0.2.0-alpha.1"),
        ("FerrumData", "--app-data", "0.3.0-alpha.1"),
        ("FerrumTerm", "--app-term", "0.2.0-alpha.1"),
    ];
    window(
        "Ferrum Toolbox",
        "tb",
        html! {
            div class="tb__banner" {
                span { "Ferrum Toolbox 0.5.0 is available" }
                span class="tb__btn tb__btn--self" { "Update Toolbox" }
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
