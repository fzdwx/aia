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

this repository currently runs as a library-first rust workspace with a web-first application shell:

- `crates/aia-config`: shared application defaults for workspace-local paths, server defaults, and stable identifiers
- `crates/agent-core`: core domain types for models, tools, and portable tool specs
- `crates/session-tape`: append-only tape with flat entries (`{id, kind, payload, meta, date}`), anchors, handoff events, query slicing, fork/merge, and jsonl replay snapshots
- `crates/agent-runtime`: runtime orchestration for models, tools, sessions, compression, cancellation, and event flow
- `crates/channel-bridge`: shared channel models, configured profile persistence facade, adapter catalog, session-binding, and turn-preparation abstractions for multi-channel ingress
- `crates/channel-feishu`: Feishu-specific websocket protocol, reply orchestration, and channel adapter implementation
- `crates/provider-registry`: local provider profiles, active selection, and serialization model
- `crates/openai-adapter`: real model adapter layer covering both Responses-style and OpenAI-compatible Chat Completions HTTP interfaces
- `crates/agent-store`: local SQLite-backed session + trace persistence
- `apps/agent-server`: axum HTTP+SSE server bridging the shared runtime to clients and hosting only thin channel host registration/adaptation
- `apps/web`: the primary web interface shell built with React + Vite+

`apps/web` is the primary client direction. `apps/agent-server` remains a thin application shell focused on HTTP + SSE bridging instead of re-owning agent logic.

## current behavior

- provider state persists under `.aia/store.sqlite3`
- channel profile state persists under `.aia/store.sqlite3`
- session replay data appends to `.aia/session.jsonl`
- local SQLite state persists under `.aia/store.sqlite3`
- server startup derives these defaults through `crates/aia-config`
- server restores remembered provider bindings and falls back to bootstrap when no valid binding exists
- one user turn now runs as an internal multi-step loop: model → tool execution/results → model continuation
- tool failures are recorded as structured facts instead of crashing the whole session
- runtime cancellation propagates from server → runtime → provider streaming / embedded shell execution
- prompt caching is wired through the shared request path for OpenAI-compatible providers, with stable session-scoped cache keys and `24h` retention
- trace data now follows an otel-shaped local model with stable trace/span ids, local events, and real tool spans
- trace list loading now reads lightweight request summaries instead of deserializing full upstream request payloads for every row
- context compression calls now emit their own trace records and can be inspected in a dedicated compression-log view instead of being mixed into the regular trace list
- trace workbench now loads its filtered summary + page data through a single overview request; overview page results are truly item-paginated, while summary data is served from a SQLite overview-summary snapshot instead of being recomputed on every request
- feishu channels now run through a long-lived websocket bridge in `apps/agent-server`; inbound events are acknowledged quickly and the actual agent turn + reply path continues asynchronously through the existing session manager/runtime chain
- channel ingress now shares session-binding, stale-binding recovery, turn preparation, message-receipt idempotency, configured profile persistence facade, generic adapter catalog, and adapter-exposed config schema through `crates/channel-bridge`; the Feishu websocket/protocol/reply implementation now lives in `crates/channel-feishu`

## web workspace

`apps/web` is no longer a placeholder. it now contains the main workbench UI for:

- provider management
- channel management (currently Feishu only, backed by long connection ingress)
- session list and history hydration
- streaming assistant / thinking / tool output rendering
- current turn recovery and cancellation
- trace loop / span inspection
- dedicated context compression log inspection
- theme handling and frontend presentation only

the web app currently uses Vite+ tooling and a `pnpm` lockfile. see `apps/web/AGENTS.md` for local frontend workflow constraints.

## runtime + server notes

`apps/agent-server` now hosts a dedicated runtime worker that owns `AgentRuntime`, provider state, and session persistence. HTTP routes communicate with that worker through message passing for mutating operations and use shared snapshots for lightweight reads.

the `agent-server` binary now has two entry modes:

- `cargo run -p agent-server` starts the default HTTP + SSE server on `3434`
- `cargo run -p agent-server -- self` starts a dedicated terminal self-chat session with compile-time embedded `docs/self.md` installed as the session system prompt on the same runtime/session-manager path used by the server
- `cargo run -p agent-server -- self <task...>` does the same, but also sends the provided startup task as the first user-direction message
- in `self` mode, `/help`, `/status`, `/compress`, and `/handoff <name> <summary>` now reuse the existing session-manager command surface instead of shelling out or bypassing runtime state; malformed built-in commands are rejected locally instead of being forwarded as model prompts

that means:

- long shell/model turns no longer block provider or session info reads
- session tape entries append during execution instead of only at the end of a turn
- provider changes use transactional persistence so registry/runtime/tape state does not diverge
- server startup and JSON serialization failure paths are structured error paths instead of panic paths

## trace direction

trace is currently “OTel-shaped local diagnostics”, not a complete OpenTelemetry exporter:

- each agent loop is treated as a root span
- each llm request is treated as a client span
- each runtime tool execution is persisted as an internal span
- local event timelines are stored for request start, first reasoning/text delta, tool-call detection, completion, and failure
- context compression requests are also persisted as inspectable trace entries and surfaced through a dedicated compression-log view instead of only surfacing as a transient SSE notice

this keeps the semantic model clean now, while leaving exporter / collector work for later.

## project coordination

project coordination lives in:

- `AGENTS.md`: repository rules and update discipline
- `docs/requirements.md`: structured requirements and current scope boundary
- `docs/architecture.md`: crate boundaries and implementation direction
- `docs/status.md`: current project phase, progress, and next step
