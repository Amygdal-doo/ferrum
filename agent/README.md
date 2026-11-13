# FerrumChat

A Cursor-like agentic coding chat, built as two reusable crates. It is embedded
as the Agent pane in Ferrum Editor and as the Chat column in the FerrumAgentic
app.

| Crate | What it is |
|---|---|
| `crates/core`, `ferrumagent-core` | Everything except pixels: providers, the agent loop, tools, change sets, rules, MCP, ACP and storage. It has no GPUI dependency. |
| `crates/ui`, `ferrumagent-ui` | The GPUI view, `ChatView`, built on `ferrum-ui` widgets. |

The editor's working-tree snapshot (`snapshot.rs`) and hunk review
(`review.rs`) moved here from `editor/crates/core/src/agents/`.
`ferrumeditor-core` re-exports them as `ferrumeditor_core::agents::{snapshot, review}`,
so the editor's CLI-agent review keeps working unchanged.

## Embedding

```rust
ferrumagent_ui::init(cx); // once, after ferrum_ui's theme / text-input keys
let chat = ChatView::new(project_root, ChatOptions {
    path_env: Some(login_shell_path), // so npx / gemini / MCP servers are found
    ..Default::default()
}, window, cx);
cx.subscribe(&chat, |_, event, cx| match event {
    ChatEvent::OpenFile { path, line } => {}       // absolute path, 1-based line
    ChatEvent::RunInTerminal { cmd, cwd } => {}    // the "Run in terminal" button
    ChatEvent::ChangesUpdated { files } => {}      // pending review count
}).detach();
// Host context: chat.update(cx, |c, cx| c.add_mention(Mention::Selection { .. }, cx));
```

`ChatOptions::config` takes a full `SessionConfig`, for example a custom
`CommandRunner` that runs `run_command` in the host's terminal, or a mock
provider. Without it, everything loads from `~/.ferrum/agent/` and the
keychain. The session snapshots the project when it opens, which blocks, so
`ChatView` opens it in the background.

The `FerrumChat` key context has these bindings: ⏎ send, ⇧⏎ newline,
⇧Tab cycle mode, Esc (closes the picker, then any overlay, then stops the
turn), ⌘. stop, ⌘⌥N new thread, ⌘⌥J threads, ⌘⌥R changes, ⌘⇧⏎ accept all,
⌘⇧⌫ reject all. In the @-picker, ↑/↓ move and Tab or ⏎ pick.

## What works

**Providers.** All providers sit behind one streaming `Provider` trait that
emits text deltas, thinking deltas, complete tool calls, usage and the stop
reason.
- **Anthropic Messages API**: streaming SSE, tool use, `GET /v1/models`. The
  defaults are `claude-sonnet-5-5`, `claude-opus-5-5` and `claude-haiku-5-5`.
  The list is user-editable.
- **OpenAI**: Responses API (the default) or Chat Completions, both streaming
  with function tools, plus a custom base URL. Chat Completions also covers
  OpenRouter, vLLM, LM Studio and similar servers.
- **Ollama**: its OpenAI-compatible `/v1` endpoint, with models discovered
  through `/api/tags`. No key is needed.
- **API keys** come from the OS keychain (`keyring`, service
  `dev.ferrum.agent.api-key`), falling back to `ANTHROPIC_API_KEY` /
  `OPENAI_API_KEY`. `ApiKey` has a redacting `Debug` and no `Display`, and
  HTTP errors drop the URL. Keys are never written to files: `settings.json`
  holds only providers, models and URLs. The settings sheet takes a key
  through a masked input and afterwards shows only its source and a short
  hint.

**Agent loop** (`agent.rs`). The loop streams, runs tools and feeds results
back, for up to 40 model requests per turn. It supports cancellation, and a
denied tool ends the turn so the user can redirect.

**Built-in tools.** Every built-in tool is confined to the project root
(`tools/sandbox.rs`): `..` cannot climb out, absolute paths must be inside the
root, symlinks are resolved, and writes into `.git` are refused. The tools
are:
- `read_file`
- `list_dir`
- `grep` (regex plus glob, gitignore-aware)
- `edit_file` (unique search/replace, or `replace_all`)
- `write_file`
- `delete_file`
- `run_command` (captured output with a timeout; the host can swap the
  `CommandRunner`, and cards have "Run in terminal")
- `git_status`
- `git_diff`
- `write_plan` (Plan mode only)

**Permissions.** Reads are allowed. Writes, commands and MCP tools ask first.
The user can answer allow once, allow for the session, always allow (saved to
`~/.ferrum/agent/permissions.json`) or deny. The question reaches the UI as a
`SessionEvent::Permission` and appears inline on the tool card.

**Change set and checkpoints** (`changeset.rs`). Every agent write goes
through a change set against the session snapshot. That covers built-in
tools and ACP `fs/write_text_file`.
- **Review**: accept or reject per hunk, per file or everything. A review
  action is refused if the file changed since it was computed.
- **Checkpoints**: one per user turn. "Restore" on a user message puts every
  file touched since back as it was before that message. The restore can be
  undone until the next turn starts.

**Modes.**
- **Agent**: all tools are available.
- **Ask**: only read-only tools are offered, and write calls are refused.
- **Plan**: read-only tools plus `write_plan`. The plan card has a **Build**
  button, which switches to Agent and sends the plan.
- **ACP agents**: in Ask and Plan, FerrumChat refuses their writes,
  terminals and edit/execute permissions itself, and maps the mode to the
  agent's own `plan` mode when the agent offers one.

**Context** (`context.rs`).
- **@-mentions** cover a file, a folder, a host-provided selection, the git
  diff, host-provided terminal output and a URL (http(s) only, HTML reduced
  to text).
- **Picker**: fuzzy file and folder suggestions. `@path` tokens typed
  inline are recognised too.
- **Rules** load automatically:
  - `AGENTS.md`, including nested files when a file in their folder is in
    context.
  - `CLAUDE.md` and `.claude/CLAUDE.md`.
  - `.ferrum/rules/*.md`: always applied unless `alwaysApply: false`.
  - `.cursor/rules/*.mdc`: Cursor semantics, so `alwaysApply`, then `globs`;
    rules that have only a `description` are listed for the model to read.

**MCP client** (`mcp/`).
- **Transports**: stdio and streamable HTTP. HTTP answers may be JSON or SSE,
  and the `Mcp-Session-Id` and `MCP-Protocol-Version` headers are sent.
- **Config**: `~/.ferrum/agent/mcp.json` in the `{"mcpServers": {...}}`
  shape that Claude and Cursor use. `${VAR}` expansion works in env,
  headers and URL.
- **Behaviour**: tools are paginated and named `mcp__<server>__<tool>`.
  Servers connect on the first built-in-agent turn, and their status shows
  in the UI. The same servers are forwarded to ACP agents in `session/new`.
- **Not supported**: project-level MCP configs are deliberately ignored, so
  that a cloned repo cannot start processes.

**ACP client** (`acp/`), protocol v1, checked against
agentclientprotocol.com in Nov 2026.
- **Client to agent**: `initialize`, `session/new` or `session/load` (when
  the agent advertises `loadSession` and the thread has a session id),
  `session/prompt`, `session/set_mode`, and the `session/cancel`
  notification.
- **Agent to client**: `session/request_permission` (goes through the same
  policy and UI), `fs/read_text_file` (sandboxed), `fs/write_text_file`
  (goes into the change set) and `terminal/create|output|wait_for_exit|kill|release`
  (child processes with captured output).
- **Session updates** map into the same thread model: message and thought
  chunks, `tool_call` and `tool_call_update` (content, diff and terminal),
  `plan` and `usage_update`.
- **Presets**, which you can override or extend in
  `~/.ferrum/agent/agents.json` (Zed's `agent_servers` shape):
  - Claude Code: `npx -y @agentclientprotocol/claude-agent-acp`, the
    successor to `@zed-industries/claude-code-acp`.
  - Codex: `npx -y @zed-industries/codex-acp`.
  - Gemini CLI: `gemini --experimental-acp`.

**Threads.** Threads are saved per project as JSON under
`~/.ferrum/agent/threads/<project>-<hash>/`. Each holds the title (from the
first message), messages, tool calls with their output, cumulative token
usage, context size, mode, agent, plan and ACP session id. The UI lists,
opens, deletes and creates threads.

**UI** (`ChatView`).
- **Header**: thread list toggle, title, changed-file count, new thread and
  settings.
- **Messages**: streaming markdown (headings, lists, quotes, code blocks
  with copy, inline code, bold, italic, links), collapsible thinking, plan
  checklists and errors.
- **Tool cards**: status, title, inline permission buttons, output, diffs,
  "open file" links and "Run in terminal".
- **Changes panel**: per-file diff with hunks, Accept and Reject per hunk,
  per file or for everything, and "Undo restore".
- **Composer**: @-picker, context chips, queued messages (sent in order,
  removable), the Agent/Ask/Plan switch, the agent and model dropdown
  (built-in models and ACP agents), a token meter (context used against the
  window) and send/stop.
- **Settings sheet**: keys, editable models with "Fetch models", and the
  base URL.
- **Width**: the layout works down to 360px; overlays replace the column
  rather than float over it.

## Tests (no network)

`cargo test -p ferrumagent-core -p ferrumagent-ui`:

| Area | Tests |
|---|---|
| Providers | Recorded Anthropic, Chat Completions and Responses streams; request bodies; Ollama tags; SSE decoder |
| Mock provider | Scripted streaming and tool calls through `ChatSession`: edits with permission, checkpoint restore and undo, Ask mode, denial, sandbox escape refused, queueing, cancel, persistence |
| Sandbox | `..`, absolute paths, symlinked folders and files |
| Change set | Per-hunk and bulk review, checkpoints, restore and undo |
| Rules | Every format, globs, nested `AGENTS.md` |
| MCP | In-test stdio server (this test binary re-run as a server), streamable-HTTP server with SSE and session id, MCP tool through a session |
| ACP | Scripted fake agent process covering permission, fs read and write, terminal, plan, usage, Ask mode refusing writes |
| UI | Headless `ChatView` at 360px in both themes: Enter sends, permission on the card, change review, accept all, @-picker with Tab, modes, thread list, settings |

## Not built yet

- **No live network testing.** Live calls to Anthropic, OpenAI, Ollama and
  real ACP agents were not exercised here; the wire formats are tested
  against recorded and scripted streams.
- **Missing protocol features**:
  - Prompt caching, extended-thinking parameters, images or attachments in
    prompts.
  - MCP OAuth, resources, prompts and sampling.
  - ACP `authenticate`, auth-method UI, slash-command lists
    (`available_commands_update` is ignored), and the ACP Registry.
- **Checkpoints**:
  - They live in memory for the session; they are not persisted across app
    restarts.
  - Restore puts files back but keeps the conversation; you cannot edit and
    resend a message from it.
  - Files changed by shell commands show up in the review but are not
    covered by checkpoints.
- **Context**: no symbol and diagnostics mentions, no auto-compaction or
  `/compact`, and no auto-generated titles (the title is the first line of
  the first message).
- **Agent output**:
  - `run_command` runs as a captured child process. A host that wants it in
    a visible terminal passes its own `CommandRunner`.
  - ACP terminals are captured too and are not mirrored into FerrumTerm.
- **UI**:
  - No syntax highlighting in code blocks.
  - No text selection in messages; code blocks have a copy button.
  - No image paste.
  - No inline editor diffs; review happens in the panel.
- **Parallel threads**: one running turn per `ChatSession`, and no worktree
  isolation.
