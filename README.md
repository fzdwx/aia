# aia agent
i just wanted an aia agent.

- a nice tui
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
- `crates/openai-adapter`: the first real model adapter, targeting responses-style http interfaces
- `apps/agent-cli` (binary `aia`): a tiny runnable shell used to verify the core boundaries

`agent-cli` is now split into startup wiring, provider setup, a shared driver layer, loop driving, rendering, and tui modules. when running in a real terminal it prefers a minimal tui; provider selection and the first question now happen inside the tui startup state machine, and the current session remembers the last provider binding unless the user actively presses `F2` during startup to replace it. in non-terminal environments it falls back to the plain text loop, but both paths now share the same driver protocol.

the shared driver boundary has also been tightened so it no longer leaks cli-specific error types or pre-stringified errors into the reusable turn-driving path.

on shutdown, the shared driver now only finalizes and persists session state; it no longer injects a hard-coded handoff summary on exit.

the current tui message flow now renders markdown content into terminal lines and keeps a single scrollable message list with auto-follow unless the user scrolls upward.

on startup, `aia` now enters a terminal provider flow: create a provider, select a saved provider, or fall back to the local bootstrap model. local provider data is stored in `.aia/providers.json`, and `.aia/` is ignored to reduce accidental commits.

session replay data is stored separately in `.aia/session.jsonl` as jsonl snapshots of flat tape entries (`{id, kind, payload, meta, date}`). the tape core now uses a single flat entry model aligned with republic, bub, and tape.systems — each entry carries its kind as a string, its payload as json, and optional metadata including run_id for turn grouping. legacy session files in the old `{id, fact, date}` format are auto-converted on load. the shared tape core also exposes named-tape storage, query slicing, and append-only fork/merge semantics.

see `docs/architecture.md` for the first-phase architecture and why the project starts from reusable libraries instead of a premature ui shell.

project coordination lives in:

- `AGENTS.md`: repository rules and update discipline
- `docs/requirements.md`: structured requirements and current scope boundary
- `docs/status.md`: current project stage and next step
