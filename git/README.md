# FerrumGIT

A local-first, GitKraken/Tower-style Git client written in Rust: a native GPUI desktop app
(the UI framework of Zed, from the `../ide` submodule), the shared `ferrum-ui` crate
(`../shared/ui`), the system `git` binary, SQLite. No backend, no account, no Electron, no webview.
Part of the Ferrum monorepo next to [FerrumData](../data) and [FerrumAPI](../api): same toolchain
and the same Ferrum theme, fonts and icons.

> **Status: M0, first vertical slice.** "Tested" means automated tests against the real
> `git` binary (temp repos plus a local bare remote, so no network). The UI has been
> smoke-launched on a demo repository, but it has not been clicked through by a person since the
> move from Dioxus to GPUI.

## Per-project SSH and identity (profiles)
A **profile** = commit identity (name + email) + SSH authentication (system ssh-agent/config,
or a specific private key). Each repository is assigned one profile (toolbar → **Profile**).

- Commits, merges, pulls and tags get `GIT_AUTHOR_*`/`GIT_COMMITTER_*` from the profile.
  These env vars take precedence over every git config level.
- Fetch, pull, push, clone and remote deletes run with
  `GIT_SSH_COMMAND="ssh -i '<key>' -o IdentitiesOnly=yes"`, so only that key is offered.
- Key passphrases are kept in the **OS keychain** (service `dev.ferrumgit.ssh-passphrase`).
  The FerrumGIT binary is its own `SSH_ASKPASS`: the passphrase goes to a 0600 temp file for the
  duration of one command and is answered only for passphrase prompts. ssh never prompts on a TTY.
- Profile editor: pick a key from `~/.ssh` or browse for one, generate a new ed25519 key, show and
  copy the public key, and **Test connection** (`ssh -T`; trusts a first-seen host key with `accept-new`).
- HTTPS remotes keep using git's credential helper (e.g. osxkeychain).

## What works today
| Area | State |
|------|-------|
| Workspace, pinned toolchain (1.99.1), workspace lints, `cargo git-ui` / `cargo lint` | ✅ |
| Open / clone (with profile, cancellable) / init; repository tabs restored after restart; recent list | ✅ tested (service level) |
| Commit graph: lane layout (straight lanes, merges, branch-offs, colours), ref badges, WIP row, virtualized, 2000 commits + "load more" | ✅ layout unit-tested; rendered on demo repo |
| Working copy: unstaged / staged / conflicted lists, stage / unstage / discard file or all | ✅ tested |
| Diff viewer; **stage / unstage / discard per hunk and per selected lines** | ✅ tested end-to-end with `git apply` |
| **Side-by-side diffs**: Unified / Split switch in the diff and commit-details headers, remembered in settings; line staging works in both | ⚠️ line pairing + setting persistence unit-tested; UI built but not clicked through |
| **File history** (commits touching a file, follows renames, from the diff header or a file in commit details) and **blame** (short id, author, relative date per run of lines; uncommitted lines marked; clicking selects the commit in the graph; "Blame" at any history entry) | ⚠️ git layer + parsers tested against real repos; UI built but not clicked through |
| **Commit search**: toolbar search matches summary, author name/email and SHA prefix instantly, plus full message bodies via `git log --grep`; matches highlighted in the graph, ↑/↓ (Enter = next) select and scroll, optional "only matches" filter | ⚠️ matching + navigation unit-tested, `--grep` tested; UI built but not clicked through |
| **Undo** (toolbar) of the last local commit / amend (soft reset, changes stay staged, not offered once pushed) or checkout (switch back), read from the HEAD reflog and re-checked before running; discards, resets, merges, rebases, pulls are never undoable | ✅ tested (service level); UI not clicked through |
| Commit with summary + description, amend (pre-fills last message), ⌘↩ | ✅ tested |
| Branches: create (from anything), checkout, checkout remote (tracking), rename, delete, delete on remote, merge, rebase | ✅ tested |
| Commits: checkout detached, cherry-pick, revert (merge commits use `-m 1`), reset soft/mixed/hard, create branch/tag here, copy SHA | ✅ tested |
| Merge/rebase/cherry-pick/revert in progress: banner, take ours/theirs, mark resolved, continue, abort | ✅ tested |
| Tags: lightweight/annotated, push, delete local/remote | ✅ tested |
| Stash: save (message, include untracked), apply, pop, drop, view stash diff | ✅ tested |
| Remotes: fetch (all / one, prune), pull (ff-only / merge / rebase), push (auto-sets upstream; force-with-lease), add/remove remote | ✅ tested against local bare remote |
| Profiles with per-repo assignment, keychain passphrases, askpass, key generation, test connection | ✅ env/identity tested; real SSH servers not tested automatically |
| **Pull / merge requests (GitHub, GitLab, GitHub Enterprise, self-managed GitLab)**: forge detected from the remote (SSH, `ssh://` with ports, HTTPS, nested GitLab groups, `insteadOf` aliases); per-repo override of forge type, API base URL and remote (sidebar → *Pull Requests* → ⚙, or Repository settings → *Token & forge…*) | ✅ URL parsing / detection unit-tested; settings persisted and tested |
| Tokens: a personal access token per forge host in the **OS keychain** (service `dev.ferrumgit.forge-token`, account = host), or for GitHub the `gh` CLI login (`gh auth token --hostname <host>`, read on demand and kept only in memory). Onboarding lists the scopes (GitHub `repo`, GitLab `api`) and opens the token page | ✅ keychain/session store + gh source tested with fakes (no real token or CLI in tests); dialog built, not clicked through |
| PR list: open PRs/MRs with title, number, author, branch → base, draft, labels, review decision and CI status, updated time; filter All / Mine / Review requested; open in browser | ✅ REST mapping + status aggregation tested against recorded GitHub v3 / GitLab v4 JSON fixtures; UI built, not clicked through |
| PR detail: description (basic Markdown), conversation (comments, reviews, line comments, GitLab system notes), commits, CI checks (GitHub check runs + commit statuses, GitLab pipeline jobs), **changed files in the existing diff view** — the PR head (`refs/pull/<n>/head` / `refs/merge-requests/<n>/head`) and base are fetched into `refs/ferrumgit/pr/<n>/…` (hidden from the graph) with the repo's profile, and diffed against the merge base | ✅ fetch + merge-base diff tested end to end against a local bare remote; Markdown parser unit-tested; UI built, not clicked through |
| PR actions: check out (same-project branch → tracking branch of the same name; fork → `pr-<n>-<branch>`; existing branch fast-forwarded), create from the current branch (title, description, base, draft; pushes first when needed), comment, review (approve / request changes / comment), **merge** with a confirmation and only the methods the project allows (head SHA must be unchanged) | ✅ checkout/create tested end to end (local bare remote + fake API); review/merge payloads fixture-tested; UI not clicked through |
| API errors and rate limits: 401 explains the needed scopes, 403/404/409/422 show the forge's message; rate-limit headers are tracked per API, an exhausted limit is reported with the reset time and not retried, and lists skip per-PR status lookups when fewer than 100 requests remain | ✅ unit + fixture tested |
| Auto-refresh when the repo changes outside the app (3 s fingerprint poll) | ⚠️ implemented, not hand-tested |
| "One Dark" theme, resizable + persisted panels | ✅ (light theme not available yet) |

## Not built yet
Redo, and undo beyond the last commit / amend / checkout (merges, rebases, cherry-picks, resets);
interactive rebase, submodules, LFS, worktrees, GPG/SSH commit signing UI (signing configured in
git config still works), issue integrations; for pull requests: inline (per-line) review comments
and replying to threads, editing / closing PRs, creating PRs whose head is in a fork, pagination
beyond the first 50 PRs and 100 commits / comments / checks, GitHub team review requests in the
"Review requested" filter, and the GitLab "requested changes" reviewer state (GitLab REST v4 has no
call for it on every edition: *Request changes* withdraws your approval and posts the comment);
search beyond the loaded history
(matches are limited to the commits in the graph; "Load more" extends it), blame of a line's
previous revision ("blame parent"), horizontal scrolling of long lines in blame and split diffs
(they are clipped), merge-conflict editor (use ours/theirs or edit the file, then *Resolved*),
progress percentages for network ops, and packaging and signing.

## Prerequisites
- Rust via rustup; `rust-toolchain.toml` installs **1.99.1**.
- `git` ≥ 2.32 and OpenSSH on `PATH` (`ssh`, `ssh-keygen`).
- Optional: the GitHub CLI (`gh`) to reuse its login for pull requests.
- macOS: Xcode (GPUI compiles Metal shaders). Linux: the GPUI/Zed build dependencies
  (`../ide/script/linux` installs them).

## Commands
Run from the monorepo root (`..`):
```bash
cargo git-ui                      # run the app (cargo run -p ferrumgit-desktop)
cargo test -p 'ferrumgit-*'       # unit + integration tests (real git, temp dirs, no network)
cargo clippy -p ferrumgit-ui -p ferrumgit-desktop --all-targets -- -D warnings
cargo fmt -p ferrumgit-ui -p ferrumgit-desktop

FERRUMGIT_DATA_DIR=/tmp/fg cargo git-ui        # isolated local state
cargo test -p ferrumgit-credentials -- --ignored   # real OS keychain round trip
cargo test -p ferrumgit-forge     # GitHub / GitLab mapping from recorded fixtures (no network)
```

### Styling
No CSS: views are GPUI elements styled with the Ferrum theme (`ferrum_ui::theme::ActiveTheme`)
and built from `ferrum-ui` components (buttons, labels, menus, text inputs, modal, splitter,
notices). App-only
widgets live in `crates/ui/src/components/widgets.rs`.

## Layout
```
apps/desktop/        thin binary: window, menus, data dir / askpass, and one `ferrumgit_ui::GitView`
crates/ui/           the whole interface as an embeddable view (`GitView`): root view in app.rs, embed.rs (options,
                     events, init, shared services); components/{shell (toolbar, compact toolbar, layout),sidebar,graph,
                     detail,diff,split,file_panel,dialogs,home,widgets}, state/ (plain data owned by the view),
                     commands/ (methods on the view)
crates/domain/       pure types: Commit, GitRef, WorkingStatus, FileDiff/Hunk (+ side-by-side pairing), Stash,
                     FileCommit/Blame, Undo (planned from the reflog), commit search, Profile/SshAuth, RepoEntry, RepoOp
crates/git/          git CLI runner (non-interactive env, cancellation), porcelain/log/refs/diff/blame/reflog parsers,
                     RepoOp executor, file history / blame / message grep / undo plan, hunk/line patch builder,
                     per-profile SSH env + askpass, ssh -T test
crates/graph/        commit-graph lane layout
crates/persistence/  SQLite: repos, profiles, settings, workspace
crates/forge/        GitHub REST v3 / GitLab v4 pull & merge requests behind a `Transport` trait (reqwest; JSON fixtures in tests),
                     remote URL → forge/owner/repo, rate limits, error mapping, status aggregation, basic Markdown, checkout plan
crates/credentials/  OS keychain (keyring) + session fallback for passphrases and forge tokens
crates/application/  AppServices runtime; applies each repo's profile to every operation
docs/                ARCHITECTURE
```

## Embedding FerrumGIT in another Ferrum app
`ferrumgit-ui` is the complete client (graph, hunk/line staging, blame, branches, merge/rebase,
stash, conflicts, pull requests, profiles…) as one GPUI view. The FerrumGIT app uses it with
`GitViewOptions::desktop()`; Ferrum Editor and FerrumVibe show it as a split pane:

```rust
use ferrumgit_ui::{GitView, GitViewEvent, GitViewOptions};

fn main() {
    // The host binary is ssh's SSH_ASKPASS for passphrase prompts.
    if let Some(code) = ferrumgit_ui::askpass_main() {
        std::process::exit(code);
    }
    // … inside the GPUI app, once:
    // ferrumgit_ui::init(cx);
}

// Opening the pane (window: &mut Window, cx: &mut Context<Host>):
let git = GitView::new(Some(project_root), GitViewOptions::embedded(), window, cx);
cx.subscribe(&git, |host, _, event, cx| match event {
    GitViewEvent::OpenFile { path, line } => host.open_file(path, *line, cx),
    GitViewEvent::RepoChanged { .. } | GitViewEvent::CommitCreated { .. } => {}
})
.detach();
// When the host's project changes:
git.update(cx, |git, cx| git.set_repo(&new_root, cx));
```

- `GitViewOptions { compact, show_repo_tabs, show_project_bar, open_files_in_host }`;
  `embedded()` = compact, one repository, no project bar, "Open" buttons that emit `OpenFile`.
  Compact mode works from 320 px (`MIN_WIDTH`): changes/history list over the diff, side by side
  from 720 px (`WIDE_LAYOUT_MIN_WIDTH`); branches, tags and remotes open in a drawer from the
  branch button.
- Key bindings are registered by `init` and scoped to the `FerrumGit` key context, so they only
  apply while the view has focus (quick open is left to the host without repository tabs).
- Background services (Tokio runtime, SQLite store, forge state) are one `GitServices` global per
  process, started by the first view (`init_with_services` passes pre-started ones). Every view
  shares them; each view polls only its own repository. Embedded views never overwrite the
  FerrumGIT app's saved tabs.
- The view does not grab focus; the host focuses it via `Focusable`.

## Local data
`~/Library/Application Support/dev.FerrumGIT.FerrumGIT/ferrumgit.db` (OS-appropriate elsewhere).
It holds repository paths, profiles (key *paths*, never keys or passphrases), per-repo forge
overrides, the token *source* per forge host (keychain or `gh`), and UI state. Tokens live only in
the OS keychain (or in `gh`) and are never logged; HTTP request debug output redacts them.
