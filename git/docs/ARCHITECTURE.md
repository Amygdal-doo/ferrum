# Architecture

One desktop process; no server. Same shape as FerrumData and FerrumAPI.

```
apps/desktop (window) / other Ferrum apps (split pane)
   ▼
crates/ui (GPUI foreground thread; embeddable root view `GitView`)
   │ GitView::spawn_run(...) ──► AppServices::spawn (dedicated Tokio runtime, 2 workers)
   ▼
crates/application   AppServices · add/clone/init · load (snapshot + graph) · run_op(entry, profile, RepoOp)
   │            │              │                 │
   ▼            ▼              ▼                 ▼
crates/git   crates/graph   crates/persistence  crates/credentials
 (git CLI,    (lane          (SQLite)            (OS keychain)
  parsers,     layout)
  SSH env)
   ▲            ▲              ▲
crates/domain ◄── every crate (pure types, no I/O)
```

## Why the git CLI (not libgit2)
Same choice as Tower: the user's config, hooks, signing, credential helpers,
`~/.ssh/config`, ssh-agent and every key type work unchanged. Output is read in machine formats
(`status --porcelain=v2 -z`, `log -z --format` with `%x1f` separators, `for-each-ref` with `%00`)
and parsed by pure, unit-tested functions in `crates/git/src/parse`.

Every invocation runs with `GIT_TERMINAL_PROMPT=0`, `GIT_EDITOR=true`, `GIT_OPTIONAL_LOCKS=0`,
`LC_ALL=C`, stdin closed (or a piped patch/message), and `kill_on_drop`. Network commands take a
`CancellationToken`. User-supplied names and revisions that start with `-` are rejected before
they reach argv.

## Operation flow
```
UI → RepoOp (domain enum, e.g. Push { remote, force_with_lease })
  └─ application::run_op(entry, profile, op, cancel)
       identity = profile.author name/email → GIT_AUTHOR_*/GIT_COMMITTER_*
       op.is_network() → NetEnv: GIT_SSH_COMMAND (-i key, IdentitiesOnly), SSH_ASKPASS=self,
                         passphrase (keychain) → 0600 temp file for this command only
  └─ git::Repo::execute(op, ctx) → one or a few git commands
  └─ UI reloads the snapshot (status, refs, stashes, remotes, log) → graph::layout
```

## Graph layout
Commits come in `--date-order` (children before parents). Each lane holds the commit it expects
next. A commit takes the first lane waiting for it, and the other waiting lanes converge into it. Its first
parent continues straight down in the same lane; extra parents join an existing lane or open the
first free slot. Lanes never shift sideways, so branch lines stay straight. Each row carries `up` and `down`
segments that the UI draws as SVG.

## Hunk and line staging
`git::patch::hunk_patch` rebuilds a single-hunk patch from the parsed diff, keeping only the
selected lines: unselected `+` lines are dropped and unselected `-` lines become context, with the reverse
rule for reverse patches. It is applied with `git apply --recount [--cached] [--reverse]`.

## Secrets
Passphrases live only in the OS keychain, and in a short-lived 0600 temp file while a command runs.
They never reach SQLite or the logs. Error text is passed through `sanitize`, which strips `user:token@` from
URLs. Logs hold repo ids, operation names and exit codes, never file contents or messages.
