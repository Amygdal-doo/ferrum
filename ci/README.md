# ci — the library behind Ferrum Editor's CI page

`crates/core` (`ferrumci-core`) is a UI-free Rust library for GitHub Actions
and pull-request checks. **Ferrum Editor** uses it for its CI page (⇧⌘I, or
the CI button of the Editor | Git | CI | Logs switch); see the *CI* section of
[`editor/README.md`](../editor/README.md) for what the page does and how to
use it. There is no separate CI app any more: the former FerrumCI desktop app
moved into the editor (`editor/apps/desktop/src/ci/`).

## What the library provides

| Module | What |
| --- | --- |
| `actions` | The Actions REST endpoints on FerrumGIT's forge `Api`: runs (with filters), a run, jobs, job logs, workflows, re-run (all / failed), cancel, dispatch, branches, file contents |
| `cache` | `EtagTransport`: GETs use `ETag` / `If-None-Match`, so unchanged answers are `304`s that do not count against the rate limit |
| `logs` + `ansi` | Job log parsing: timestamps stripped, ANSI colours, `##[group]` folding (quiet groups start folded), `##[error]` / `##[warning]` and compiler-style `error:` lines, search |
| `dispatch` | `workflow_dispatch` inputs read from the workflow file (string, number, boolean, choice, environment), with required and choice checks |
| `auth` | Token lookup, in this order: FerrumGIT's keychain entry (service `dev.ferrumgit.forge-token`, account = host), `gh auth token --hostname <host>`, `GH_TOKEN` / `GITHUB_TOKEN` (`GH_ENTERPRISE_TOKEN` / `GITHUB_ENTERPRISE_TOKEN` on Enterprise hosts); saving a pasted token to the same keychain entry |
| `repo` | A folder's forge repository: the `origin` remote (else the first), read from `.git/config` (worktrees too); GitHub, GitLab (reported as unsupported) or none; recent Ferrum projects on GitHub |
| `watch` | Poll intervals (12 s while something runs, 60 s otherwise, slower near the rate limit, paused until the reset when it is used up) and finished-run detection for "notify when done" |
| `format` | Durations, relative times, short SHAs |
| `Client` | Everything the page talks to for one repository, including the pull-request listing with combined check status |

It depends on `ferrumgit-forge`, `ferrumgit-credentials` and
`ferrumgit-domain`, and does not modify them. The forge layer provides the
HTTP transport, auth headers, rate-limit tracking, error messages, remote
parsing and the PR listing. Tokens are `Secret`s: they are redacted from
`Debug` output and never logged or displayed.

## Tests

```bash
cargo test -p ferrumci-core
cargo test -p ferrumeditor-desktop tests_ci   # the editor's CI page, headless
```

The tests make no network calls. `crates/core/tests/actions.rs` replays recorded
GitHub responses from `crates/core/tests/fixtures`. The editor's
`tests_ci.rs` runs the real CI page and its runtime headlessly against the same
fixtures, with an in-memory token store and notifier; it opens no window and
touches neither the keychain nor `~/.ferrum/projects.json`.

## Limitations

- GitHub only: no GitLab pipelines.
- A job's log shows once the job finishes. For a running job, **Open on GitHub**
  shows the live log.
- The run list shows the newest 30 runs that match the filters (no paging),
  and jobs are read from the latest attempt only.
- The ref picker lists the first 100 branches; tags can't be picked.
- Notifications use `osascript`, so macOS shows them under Script Editor.
  On other platforms, only the in-app notice appears.
