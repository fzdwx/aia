# like agent
i just wanted a like agent.

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

just a normal, like agent.

## current bootstrap

this repository now starts with a library-first rust workspace:

- `crates/agent-core`: core domain types for models, tools, and portable tool specs
- `crates/session-tape`: append-only fact tape with typed entries, anchors, handoff events, tool facts, anchor-based view rebuild, and jsonl replay snapshots
- `crates/agent-runtime`: minimal runtime that composes models, tools, and session state
- `crates/provider-registry`: stores local provider profiles and active selection
- `crates/openai-adapter`: the first real model adapter, targeting responses-style http interfaces
- `apps/agent-cli` (binary `like`): a tiny runnable shell used to verify the core boundaries

`agent-cli` is now split into startup wiring, provider setup, loop driving, rendering, and tui modules. when running in a real terminal it prefers a minimal tui; provider selection and the first question now happen inside the tui startup state machine, and the current session remembers the last provider binding unless the user actively presses `F2` during startup to replace it. in non-terminal environments it falls back to the plain text loop.

on startup, `like` now enters a terminal provider flow: create a provider, select a saved provider, or fall back to the local bootstrap model. local provider data is stored in `.like/providers.json`, and `.like/` is ignored to reduce accidental commits.

session replay data is stored separately in `.like/session.jsonl` as jsonl snapshots of tape entries.

see `docs/architecture.md` for the first-phase architecture and why the project starts from reusable libraries instead of a premature ui shell.

project coordination lives in:

- `AGENTS.md`: repository rules and update discipline
- `docs/requirements.md`: structured requirements and current scope boundary
- `docs/status.md`: current project stage and next step
