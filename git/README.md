# FerrumGIT

A local-first, GitKraken/Tower-style Git client written in Rust: a native GPUI desktop app
(the UI framework of Zed, taken from FerrumIDE in `../ide`), the shared `ferrum-ui` crate
(`../shared/ui`), the system `git` binary, SQLite. No backend, no account, no Electron, no webview.
Part of the Ferrum monorepo next to [FerrumData](../data) and [FerrumAPI](../api): same toolchain
and the same FerrumIDE theme, fonts and icons.

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
| Auto-refresh when the repo changes outside the app (3 s fingerprint poll) | ⚠️ implemented, not hand-tested |
| FerrumIDE "One Dark" theme, resizable + persisted panels | ✅ (light theme not available yet) |

## Not built yet
Redo, and undo beyond the last commit / amend / checkout (merges, rebases, cherry-picks, resets);
interactive rebase, submodules, LFS, worktrees, GPG/SSH commit signing UI (signing configured in
git config still works), PR/issue integrations (GitHub/GitLab), search beyond the loaded history
(matches are limited to the commits in the graph; "Load more" extends it), blame of a line's
previous revision ("blame parent"), horizontal scrolling of long lines in blame and split diffs
(they are clipped), merge-conflict editor (use ours/theirs or edit the file, then *Resolved*),
progress percentages for network ops, and packaging and signing.

## Prerequisites
- Rust via rustup; `rust-toolchain.toml` installs **1.99.1**.
- `git` ≥ 2.32 and OpenSSH on `PATH` (`ssh`, `ssh-keygen`).
- macOS: Xcode (GPUI compiles Metal shaders). Linux: the GPUI/Zed build dependencies
  (`../ide/script/linux` installs them).

## Commands
Run from the monorepo root (`..`):
```bash
cargo git-ui                      # run the app (cargo run -p ferrumgit-desktop)
cargo test -p 'ferrumgit-*'       # unit + integration tests (real git, temp dirs, no network)
cargo clippy -p ferrumgit-desktop --all-targets -- -D warnings
cargo fmt -p ferrumgit-desktop

FERRUMGIT_DATA_DIR=/tmp/fg cargo git-ui        # isolated local state
cargo test -p ferrumgit-credentials -- --ignored   # real OS keychain round trip
```

### Styling
No CSS: views are GPUI elements styled with the FerrumIDE theme (`theme::ActiveTheme`) and built
from Zed's `ui` components plus `ferrum-ui` (text inputs, modal, splitter, notices). App-only
widgets live in `apps/desktop/src/components/widgets.rs`.

## Layout
```
apps/desktop/        GPUI app: root view in app.rs; components/{shell,sidebar,graph,detail,diff,split,file_panel,dialogs,home,widgets},
                     state/ (plain data owned by the root view), commands/ (methods on the root view)
crates/domain/       pure types: Commit, GitRef, WorkingStatus, FileDiff/Hunk (+ side-by-side pairing), Stash,
                     FileCommit/Blame, Undo (planned from the reflog), commit search, Profile/SshAuth, RepoEntry, RepoOp
crates/git/          git CLI runner (non-interactive env, cancellation), porcelain/log/refs/diff/blame/reflog parsers,
                     RepoOp executor, file history / blame / message grep / undo plan, hunk/line patch builder,
                     per-profile SSH env + askpass, ssh -T test
crates/graph/        commit-graph lane layout
crates/persistence/  SQLite: repos, profiles, settings, workspace
crates/credentials/  OS keychain (keyring) + session fallback for passphrases
crates/application/  AppServices runtime; applies each repo's profile to every operation
docs/                ARCHITECTURE
```

## Local data
`~/Library/Application Support/dev.FerrumGIT.FerrumGIT/ferrumgit.db` (OS-appropriate elsewhere).
It holds repository paths, profiles (key *paths*, never keys or passphrases) and UI state.
