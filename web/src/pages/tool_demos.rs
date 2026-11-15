//! Animated mock windows for the embedded tools: FerrumEngineer's three
//! columns, and FerrumIDE with its Git, Agent and Term panes. Same rules
//! as `feature_demos` (CSS-only, 9s loop, a still frame under reduced
//! motion). Styles: `assets/css/features.css` (`.ax*`, `.edp*`).

use maud::{Markup, html};

use super::feature_demos::window;
use super::project_demos::project_strip;

/// A column or pane header: title, a hint, and the close and ⋯ controls.
fn header(class: &str, title: &str, hint: &str) -> Markup {
    html! {
        p class=(class) {
            b { (title) }
            em { (hint) }
            span aria-hidden="true" { "⋯ ✕" }
        }
    }
}

/// FerrumEngineer: the chat edits a file, the git column shows the change at
/// once, and Run in terminal runs the tests in the terminal column.
pub fn vibe() -> Markup {
    window(
        "FerrumEngineer — shop",
        "ax",
        html! {
            div class="ax__top" {
                (project_strip("shop", "~/code/shop", "main"))
                span class="ax__toggles" {
                    span class="is-on" { "Chat" } span class="is-on" { "Git" } span class="is-on" { "Term" }
                }
            }
            div class="ax__cols" {
                div class="ax__col" {
                    (header("ax__head", "Chat", "1 to review"))
                    p class="ax__msg ax__msg--you" { "Round cart totals to cents" }
                    p class="ax__tool" { span class="demo__check" { "✓" } " edit_file src/cart.ts" em { "+1 −1" } }
                    p class="ax__msg" { "cartTotal() now rounds. Run the tests?" }
                    p class="ax__run" { "Run in terminal" }
                    p class="ax__mode" { "Agent · claude-sonnet-5-5" }
                }
                div class="ax__col" {
                    (header("ax__head", "Git", "main"))
                    p class="demo__label" { "Changes" }
                    p class="ax__file" { b { "M" } " src/cart.ts" em { "+1 −1" } }
                    div class="ax__diff" {
                        p class="git__del" { "- return sum;" }
                        p class="git__add" { "+ return Math.round(sum * 100) / 100;" }
                    }
                    p class="ax__commit" { "Commit to main" }
                }
                div class="ax__col ax__col--term" {
                    (header("ax__head", "Term", "vibe:shop"))
                    p class="ax__line" { span class="t-prompt" { "shop ❯ " } "pnpm test" }
                    p class="ax__out" { "✓ cart.test.ts (4)" }
                    p class="ax__out" { "Tests  4 passed" }
                    p class="ax__out" { span class="t-prompt" { "shop ❯ " } span class="demo__caret" {} }
                }
            }
        },
    )
}

/// FerrumIDE with Git and Agent docked right, Term at the bottom, and a
/// pane's ⋯ menu offering its moves.
pub fn editor_panes() -> Markup {
    window(
        "cart.ts — shop",
        "edp",
        html! {
            div class="ax__top" {
                (project_strip("shop", "~/code/shop", "main"))
                span class="ax__toggles" {
                    span class="is-on" { "Git" } span class="is-on" { "Agent" } span class="is-on" { "Term" }
                }
            }
            div class="edp__main" {
                div class="edp__code" {
                    p class="edp__tab" { "cart.ts" }
                    p { span class="t-kw" { "export function " } span class="t-fn" { "cartTotal" } "(items) {" }
                    p { "  const sum = items.reduce(add, 0);" }
                    p class="edp__changed" { "  return " span class="t-fn" { "Math.round" } "(sum * 100) / 100;" }
                    p { "}" }
                }
                div class="edp__dock" {
                    div class="edp__pane" {
                        (header("edp__head", "Git", "1 change"))
                        p class="ax__file" { b { "M" } " src/cart.ts" em { "+1 −1" } }
                    }
                    div class="edp__pane" {
                        (header("edp__head", "Agent", "1 to review"))
                        p class="ax__msg" { "Rounded totals in cartTotal()." }
                        p class="edp__review" { span { "Accept" } span { "Reject" } }
                    }
                    ul class="edp__menu" {
                        li { "Move to Left" }
                        li { "Move to Bottom" }
                        li { "Close" }
                    }
                }
            }
            div class="edp__bottom" {
                (header("edp__head", "Terminal", "editor:shop"))
                p class="ax__line" { span class="t-prompt" { "shop ❯ " } "pnpm test " em { "4 passed" } }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_the_three_tools() {
        let ax = vibe().into_string();
        for part in ["Chat", "Git", "Term", "Run in terminal", "vibe:shop"] {
            assert!(ax.contains(part), "{part}");
        }
        let ed = editor_panes().into_string();
        for part in ["Git", "Agent", "Terminal", "Move to Left", "Move to Bottom"] {
            assert!(ed.contains(part), "{part}");
        }
    }
}
