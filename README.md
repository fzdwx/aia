# aia agent

i just wanted an aia agent.

- a good web interface
- gui support as a desktop app
- support for windows, linux, and macos
- tape.system https://tape.systems/#, from bub,https://github.com/bubbuild/bub
- no flickering
- performance-minded as an agent harness
- aware that different models have different personalities
- not benchmark-maxxed
- not absurd on cpu or ram
- mcp, tool search, subagents, async subagents, fork, and a2a built in by default, but optional
- all the tools you need for coding built in, and toggleable
- compatible harnesses and tool specs for claude and codex
- incremental compaction and handoff
- easy to use as an interface for driving other clients

just a normal, aia agent.

## current bootstrap

this repository now starts with a library-first rust workspace:

- `crates/agent-core`: core domain types for models, tools, and portable tool specs
- `crates/session-tape`: append-only tape with flat entries (`{id, kind, payload, meta, date}` aligned with republic/bub), lightweight anchors, handoff events, query slicing, fork/merge, and jsonl replay snapshots
- `crates/agent-runtime`: minimal runtime that composes models, tools, and session state
- `crates/provider-registry`: stores local provider profiles and active selection
- `crates/openai-adapter`: the first real model adapter set, now covering both responses-style and openai-compatible chat-completions-style http interfaces
- `apps/agent-server`: axum HTTP+SSE server bridging web frontend to shared runtime
- `apps/web`: the primary web interface shell built with React + Vite

`apps/web` is now the primary client direction for provider management, session timeline, and streaming interaction. `apps/agent-server` is the only application shell in the Rust workspace and stays focused on bridging HTTP + SSE into the shared runtime instead of re-owning agent logic.

the builtin coding tool contract is now intentionally short and explicit: `shell`, `read`, `write`, `edit`, `glob`, and `grep`. the shell tool keeps a stable generic name while embedding `brush` as its execution runtime.

the shared turn-driving boundary has also been tightened so it no longer leaks shell-specific error types or pre-stringified errors into the reusable runtime path.

on shutdown, the shared driver now only finalizes and persists session state; it no longer injects a hard-coded handoff summary on exit.

the runtime turn semantics now run as an internal multi-step loop instead of a single model call. one user turn can proceed as model → tool execution/results → model continuation until no further tool calls remain or a small step cap is hit. tool failures are recorded as structured failed tool outcomes plus tool-result entries on tape, so the next model step can see what failed instead of aborting the whole session immediately.

that step cap is now runtime-configurable instead of being a single fixed constant everywhere. the generic runtime keeps a conservative default safety rail, while the current web bridge uses a higher default budget suited to longer tool chains.

the stop strategy is also closer to opencode now: when a turn reaches its last allowed internal step, the runtime switches to a text-only finishing step instead of immediately failing. this preserves a hard safety rail while still giving the model one final chance to conclude cleanly without more tools.

the web turn flow keeps the same non-fatal failure policy: a failed turn is emitted through structured SSE error/status events, but the session itself stays alive so the next user input can continue.

on startup, `apps/agent-server` restores provider state from `.aia/providers.json` and `.aia/session.jsonl`, preferring the latest remembered binding and falling back to the local bootstrap model when no matching provider exists. `.aia/` is ignored to reduce accidental commits.

provider profiles now persist the exact protocol kind they use, so the server can distinguish the same endpoint/model under different wire protocols. remembered session bindings restore the correct protocol instead of guessing from name/model/base url alone.

the model-facing continuation context is no longer rebuilt only from flattened `role/content` messages. tool calls and tool results are now preserved as structured conversation items through `agent-core` → `session-tape` → `agent-runtime` → `openai-adapter`, so follow-up tool turns can be mapped back into protocol-native request shapes instead of lossy plain text.

for the OpenAI Responses path specifically, `aia` now persists a session-local continuation checkpoint from each successful response and resumes later turns with `previous_response_id` plus only the incremental local input. that means both same-turn tool outputs and later user follow-ups can continue the same remote response chain without replaying the full conversation payload each time.

session replay data is stored separately in `.aia/session.jsonl` as jsonl snapshots of flat tape entries (`{id, kind, payload, meta, date}`). the tape core now uses a single flat entry model aligned with republic, bub, and tape.systems — each entry carries its kind as a string, its payload as json, and optional metadata including run_id for turn grouping. legacy session files in the old `{id, fact, date}` format are auto-converted on load. the shared tape core also exposes named-tape storage, query slicing, and append-only fork/merge semantics.

`apps/web` is no longer a template placeholder. it now contains the primary web interface connected to the shared runtime via `apps/agent-server` (axum HTTP+SSE on port 3434). the web frontend consumes a global SSE event stream (`GET /api/events`) for real-time streaming of thinking, tool output, and assistant text, with fire-and-forget message submission (`POST /api/turn`). user messages are now rendered optimistically at submit time instead of waiting for turn completion.

`apps/agent-server` no longer runs full turns while holding a single shared app mutex. it now hosts a dedicated runtime worker that owns `AgentRuntime`, provider state, and session persistence, while HTTP routes talk to that worker over message passing and read provider snapshots without waiting for long shell / model turns to finish. that keeps the server usable as an interface layer for other clients even when a turn is running a large tool.

see `docs/architecture.md` for the first-phase architecture and why the project starts from reusable libraries instead of pushing agent logic into a client shell.

project coordination lives in:

- `AGENTS.md`: repository rules and update discipline
- `docs/requirements.md`: structured requirements and current scope boundary
- `docs/status.md`: current project stage and next step
