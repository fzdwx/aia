import { create } from "zustand"
import {
  fetchCurrentTurn,
  fetchHistory,
  fetchProviders,
  fetchSessionInfo,
  fetchSessions as apiFetchSessions,
  createSession as apiCreateSession,
  deleteSession as apiDeleteSession,
  listProviders as apiListProviders,
  switchProvider as apiSwitchProvider,
  submitTurn as apiSubmitTurn,
  cancelTurn as apiCancelTurn,
  createProvider as apiCreateProvider,
  updateProvider as apiUpdateProvider,
  deleteProvider as apiDeleteProvider,
} from "@/lib/api"
import { createIdleScheduler, type IdleCanceller, type IdleScheduler } from "@/lib/idle"
import { normalizeToolArguments } from "@/lib/tool-display"
import type {
  AppView,
  ChatState,
  ContextCompressionNotice,
  CurrentTurnSnapshot,
  ModelConfig,
  ProviderInfo,
  ProviderListItem,
  SessionListItem,
  SseEvent,
  StreamingTurn,
  TurnLifecycle,
  TurnStatus,
} from "@/lib/types"

const SESSION_HISTORY_PAGE_SIZE = 5
const INITIAL_SESSION_HISTORY_PAGE_SIZE = 1
const MAX_CACHED_SESSION_SNAPSHOTS = 24

const defaultIdleScheduler = createIdleScheduler()

type IdleHandle = number

type SessionSnapshot = {
  latestTurn: TurnLifecycle | null
  streamingTurn: StreamingTurn | null
  chatState: ChatState
  contextPressure: number | null
  lastCompression: ContextCompressionNotice | null
}

const EMPTY_SESSION_SNAPSHOT: SessionSnapshot = {
  latestTurn: null,
  streamingTurn: null,
  chatState: "idle",
  contextPressure: null,
  lastCompression: null,
}

function latestTurn(turns: TurnLifecycle[]): TurnLifecycle | null {
  return turns.length > 0 ? turns[turns.length - 1] ?? null : null
}

function snapshotFromState(state: Pick<
  ChatStore,
  | "turns"
  | "streamingTurn"
  | "chatState"
  | "contextPressure"
  | "lastCompression"
>): SessionSnapshot {
  return {
    latestTurn: latestTurn(state.turns),
    streamingTurn: state.streamingTurn,
    chatState: state.chatState,
    contextPressure: state.contextPressure,
    lastCompression: state.lastCompression,
  }
}

function mergeTurnsById(
  olderTurns: TurnLifecycle[],
  newerTurns: TurnLifecycle[]
): TurnLifecycle[] {
  const seen = new Set<string>()
  const merged: TurnLifecycle[] = []

  for (const turn of [...olderTurns, ...newerTurns]) {
    if (seen.has(turn.turn_id)) continue
    seen.add(turn.turn_id)
    merged.push(turn)
  }

  return merged
}

function currentTurnToStreamingTurn(
  current: CurrentTurnSnapshot
): StreamingTurn {
  return {
    userMessage: current.user_message,
    status: current.status,
    blocks: current.blocks.map((block) => {
      switch (block.kind) {
        case "thinking":
          return { type: "thinking", content: block.content } as const
        case "text":
          return { type: "text", content: block.content } as const
        case "tool":
          return {
            type: "tool",
            tool: {
              invocationId: block.tool.invocation_id,
              toolName: block.tool.tool_name,
              arguments: normalizeToolArguments(block.tool.arguments),
              detectedAtMs: block.tool.detected_at_ms,
              startedAtMs: block.tool.started_at_ms ?? undefined,
              finishedAtMs: block.tool.finished_at_ms ?? undefined,
              output: block.tool.output,
              completed: block.tool.completed,
              resultContent: block.tool.result_content ?? undefined,
              resultDetails: block.tool.result_details ?? undefined,
              failed: block.tool.failed ?? undefined,
            },
          } as const
      }
    }),
  }
}

function applySessionSnapshot(
  snapshot: SessionSnapshot,
  sessionHydrating: boolean
): Pick<
  ChatStore,
  | "sessionHydrating"
  | "turns"
  | "historyHasMore"
  | "historyNextBeforeTurnId"
  | "streamingTurn"
  | "chatState"
  | "contextPressure"
  | "lastCompression"
  | "error"
> {
  return {
    sessionHydrating,
    turns: snapshot.latestTurn ? [snapshot.latestTurn] : [],
    historyHasMore: false,
    historyNextBeforeTurnId: null,
    streamingTurn: snapshot.streamingTurn,
    chatState: snapshot.chatState,
    contextPressure: snapshot.contextPressure,
    lastCompression: snapshot.lastCompression,
    error: null,
  }
}

function upsertSessionSnapshot(
  snapshots: Record<string, SessionSnapshot>,
  sessionId: string | null,
  snapshot: SessionSnapshot,
  sessionOrder?: SessionListItem[]
): Record<string, SessionSnapshot> {
  if (!sessionId) return snapshots

  const nextSnapshots = {
    ...snapshots,
    [sessionId]: snapshot,
  }

  if (!sessionOrder || sessionOrder.length <= MAX_CACHED_SESSION_SNAPSHOTS) {
    return nextSnapshots
  }

  const allowedIds = new Set(
    sessionOrder
      .slice(Math.max(0, sessionOrder.length - MAX_CACHED_SESSION_SNAPSHOTS))
      .map((session) => session.id)
  )
  allowedIds.add(sessionId)

  return Object.fromEntries(
    Object.entries(nextSnapshots).filter(([id]) => allowedIds.has(id))
  )
}

function trimSessionSnapshotsToKnownSessions(
  snapshots: Record<string, SessionSnapshot>,
  sessions: SessionListItem[]
): Record<string, SessionSnapshot> {
  const allowedIds = new Set(sessions.map((session) => session.id))
  return Object.fromEntries(
    Object.entries(snapshots).filter(([id]) => allowedIds.has(id))
  )
}

type ChatStore = {
  sessions: SessionListItem[]
  activeSessionId: string | null
  sessionHydrating: boolean
  turns: TurnLifecycle[]
  historyHasMore: boolean
  historyNextBeforeTurnId: string | null
  historyLoadingMore: boolean
  streamingTurn: StreamingTurn | null
  chatState: ChatState
  provider: ProviderInfo | null
  providerList: ProviderListItem[]
  error: string | null
  view: AppView
  contextPressure: number | null
  lastCompression: ContextCompressionNotice | null
  _pendingPrompt: string | null
  _sessionSnapshots: Record<string, SessionSnapshot>
  _refreshProviderInfo: () => Promise<void>
  initialize: () => void
  handleSseEvent: (event: SseEvent) => void
  submitTurn: (prompt: string) => void
  cancelTurn: () => Promise<void>
  switchModel: (providerName: string, modelId?: string) => void
  refreshProviders: () => void
  setView: (view: AppView) => void
  createProvider: (body: {
    name: string
    kind: string
    models: ModelConfig[]
    active_model?: string
    api_key: string
    base_url: string
  }) => Promise<void>
  updateProvider: (
    name: string,
    body: {
      kind?: string
      models?: ModelConfig[]
      active_model?: string
      api_key?: string
      base_url?: string
    }
  ) => Promise<void>
  deleteProvider: (name: string) => Promise<void>
  fetchSessions: () => Promise<void>
  createSession: () => Promise<void>
  switchSession: (id: string) => Promise<void>
  loadOlderTurns: () => Promise<void>
  deleteSession: (id: string) => Promise<void>
}

let latestSessionLoadId = 0
let pendingHistoryHydrationAbort: AbortController | null = null
let pendingHistoryHydrationIdleHandle: IdleHandle | null = null
let scheduleIdleWork: IdleScheduler = defaultIdleScheduler.schedule
let cancelIdleWork: IdleCanceller = defaultIdleScheduler.cancel

export function __setIdleSchedulerForTests(
  scheduler:
    | {
        schedule: IdleScheduler
        cancel: IdleCanceller
      }
    | null
) {
  if (scheduler) {
    scheduleIdleWork = scheduler.schedule
    cancelIdleWork = scheduler.cancel
    return
  }
  scheduleIdleWork = defaultIdleScheduler.schedule
  cancelIdleWork = defaultIdleScheduler.cancel
}

function cancelPendingHistoryHydration() {
  pendingHistoryHydrationAbort?.abort()
  pendingHistoryHydrationAbort = null
  if (pendingHistoryHydrationIdleHandle != null) {
    cancelIdleWork(pendingHistoryHydrationIdleHandle)
    pendingHistoryHydrationIdleHandle = null
  }
}

function scheduleIdle(callback: () => void): IdleHandle {
  return scheduleIdleWork(callback)
}

export const useChatStore = create<ChatStore>((set, get) => {
  async function hydrateSession(id: string) {
    cancelPendingHistoryHydration()
    const loadId = ++latestSessionLoadId
    const cachedSnapshot = get()._sessionSnapshots[id] ?? EMPTY_SESSION_SNAPSHOT
    const historyPagePromise = fetchHistory({
      sessionId: id,
      limit: INITIAL_SESSION_HISTORY_PAGE_SIZE,
    })
    const currentTurnPromise = fetchCurrentTurn(id)
    const sessionInfoPromise = fetchSessionInfo(id)

    set((state) => ({
      activeSessionId: id,
      ...applySessionSnapshot(cachedSnapshot, true),
      _sessionSnapshots: upsertSessionSnapshot(
        state._sessionSnapshots,
        state.activeSessionId,
        snapshotFromState(state),
        state.sessions
      ),
    }))

    sessionInfoPromise
      .then((info) => {
        if (loadId !== latestSessionLoadId) return
        set((state) => {
          const snapshot = {
            ...(state._sessionSnapshots[id] ?? cachedSnapshot),
            contextPressure: info.pressure_ratio,
          }
          return {
            contextPressure: info.pressure_ratio,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              id,
              snapshot,
              state.sessions
            ),
          }
        })
      })
      .catch(() => {})

    try {
      const [historyPage, currentTurn] = await Promise.all([
        historyPagePromise,
        currentTurnPromise,
      ])

      if (loadId !== latestSessionLoadId) {
        return
      }

      const hydratedSnapshot: SessionSnapshot = {
        latestTurn: latestTurn(historyPage.turns),
        streamingTurn: currentTurn ? currentTurnToStreamingTurn(currentTurn) : null,
        chatState: currentTurn ? "active" : "idle",
        contextPressure: get().contextPressure,
        lastCompression: null,
      }

      set((state) => ({
        ...applySessionSnapshot(hydratedSnapshot, false),
        _sessionSnapshots: upsertSessionSnapshot(
          state._sessionSnapshots,
          id,
          hydratedSnapshot,
          state.sessions
        ),
      }))

      if (
        INITIAL_SESSION_HISTORY_PAGE_SIZE < SESSION_HISTORY_PAGE_SIZE &&
        historyPage.has_more &&
        historyPage.next_before_turn_id
      ) {
        const beforeTurnId = historyPage.next_before_turn_id
        const abortController = new AbortController()
        pendingHistoryHydrationAbort = abortController
        pendingHistoryHydrationIdleHandle = scheduleIdle(() => {
          pendingHistoryHydrationIdleHandle = null
          void fetchHistory({
            sessionId: id,
            beforeTurnId,
            limit: SESSION_HISTORY_PAGE_SIZE - INITIAL_SESSION_HISTORY_PAGE_SIZE,
            signal: abortController.signal,
          })
            .then((olderHistoryPage) => {
              if (
                abortController.signal.aborted ||
                loadId !== latestSessionLoadId ||
                get().activeSessionId !== id
              ) {
                return
              }

              const existingTurns = get().turns
              const turns = mergeTurnsById(olderHistoryPage.turns, existingTurns)
              const nextSnapshot: SessionSnapshot = {
                latestTurn: latestTurn(turns),
                streamingTurn: get().streamingTurn,
                chatState: get().chatState,
                contextPressure: get().contextPressure,
                lastCompression: get().lastCompression,
              }
              set((state) => ({
                turns,
                historyHasMore: olderHistoryPage.has_more,
                historyNextBeforeTurnId: olderHistoryPage.next_before_turn_id,
                _sessionSnapshots: upsertSessionSnapshot(
                  state._sessionSnapshots,
                  id,
                  nextSnapshot,
                  state.sessions
                ),
              }))
            })
            .catch((error: unknown) => {
              if (
                error instanceof DOMException &&
                error.name === "AbortError"
              ) {
                return
              }
            })
            .finally(() => {
              if (pendingHistoryHydrationAbort === abortController) {
                pendingHistoryHydrationAbort = null
              }
            })
        })
      }
    } catch {
      if (loadId !== latestSessionLoadId) {
        return
      }
      set({ sessionHydrating: false })
    }
  }

  return {
    sessions: [],
    activeSessionId: null,
    sessionHydrating: false,
    turns: [],
    historyHasMore: false,
    historyNextBeforeTurnId: null,
    historyLoadingMore: false,
    streamingTurn: null,
    chatState: "idle",
    provider: null,
    providerList: [],
    error: null,
    view: "chat",
    contextPressure: null,
    lastCompression: null,
    _pendingPrompt: null,
    _sessionSnapshots: {},

    _refreshProviderInfo: async () => {
      const provider = await fetchProviders()
      set({ provider })
    },

    initialize: () => {
      get()
        ._refreshProviderInfo()
        .catch(() => {})

      apiFetchSessions()
        .then((sessions) => {
          const activeId = sessions[0]?.id ?? null
          set({ sessions, activeSessionId: activeId })
          if (activeId) {
            void hydrateSession(activeId)
          }
        })
        .catch(() => {})

      get().refreshProviders()
    },

    handleSseEvent: (event: SseEvent) => {
      const activeId = get().activeSessionId

      function findToolBlockIndex(blocks: StreamingTurn["blocks"], invocationId: string) {
        for (let i = blocks.length - 1; i >= 0; i -= 1) {
          const block = blocks[i]
          if (block?.type === "tool" && block.tool.invocationId === invocationId) {
            return i
          }
        }
        return -1
      }

      function setStreamingTurnForActiveSession(
        updater: (streamingTurn: StreamingTurn) => StreamingTurn | null
      ) {
        set((state) => {
          if (!activeId || !state.streamingTurn) return state
          const nextStreamingTurn = updater(state.streamingTurn)
          if (!nextStreamingTurn) return state
          return {
            streamingTurn: nextStreamingTurn,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              activeId,
              {
                ...(state._sessionSnapshots[activeId] ?? EMPTY_SESSION_SNAPSHOT),
                streamingTurn: nextStreamingTurn,
              },
              state.sessions
            ),
          }
        })
      }

      switch (event.type) {
        case "status": {
          if (event.data.session_id !== activeId) break

          const status = event.data.status as TurnStatus
          if (status === "waiting") {
            const prev = get().streamingTurn
            if (prev) {
              set((state) => {
                const nextStreamingTurn = { ...prev, status: "waiting" as const }
                return {
                  _pendingPrompt: null,
                  chatState: "active",
                  streamingTurn: nextStreamingTurn,
                  _sessionSnapshots: upsertSessionSnapshot(
                    state._sessionSnapshots,
                    activeId,
                    {
                      ...(state._sessionSnapshots[activeId ?? ""] ??
                        EMPTY_SESSION_SNAPSHOT),
                      streamingTurn: nextStreamingTurn,
                      chatState: "active",
                    },
                    state.sessions
                  ),
                }
              })
            } else {
              const prompt = get()._pendingPrompt ?? ""
              set((state) => {
                const nextStreamingTurn: StreamingTurn = {
                  userMessage: prompt,
                  status: "waiting",
                  blocks: [],
                }
                return {
                  _pendingPrompt: null,
                  chatState: "active",
                  streamingTurn: nextStreamingTurn,
                  _sessionSnapshots: upsertSessionSnapshot(
                    state._sessionSnapshots,
                    activeId,
                    {
                      ...(state._sessionSnapshots[activeId ?? ""] ??
                        EMPTY_SESSION_SNAPSHOT),
                      streamingTurn: nextStreamingTurn,
                      chatState: "active",
                    },
                    state.sessions
                  ),
                }
              })
            }
          } else {
            const prev = get().streamingTurn
            if (prev) {
              set((state) => {
                const nextStreamingTurn = { ...prev, status }
                return {
                  streamingTurn: nextStreamingTurn,
                  _sessionSnapshots: upsertSessionSnapshot(
                    state._sessionSnapshots,
                    activeId,
                    {
                      ...(state._sessionSnapshots[activeId ?? ""] ??
                        EMPTY_SESSION_SNAPSHOT),
                      streamingTurn: nextStreamingTurn,
                    },
                    state.sessions
                  ),
                }
              })
            }
          }
          break
        }
        case "stream": {
          if (event.data.session_id !== activeId) break

          const data = event.data
          const prev = get().streamingTurn
          if (!prev) break

          const blocks = [...prev.blocks]

          if (data.kind === "thinking_delta") {
            const last = blocks[blocks.length - 1]
            if (last && last.type === "thinking") {
              blocks[blocks.length - 1] = {
                ...last,
                content: last.content + data.text,
              }
            } else {
              blocks.push({ type: "thinking", content: data.text })
            }
            setStreamingTurnForActiveSession((current) => ({
              ...current,
              blocks,
            }))
          } else if (data.kind === "text_delta") {
            const last = blocks[blocks.length - 1]
            if (last && last.type === "text") {
              blocks[blocks.length - 1] = {
                ...last,
                content: last.content + data.text,
              }
            } else {
              blocks.push({ type: "text", content: data.text })
            }
            setStreamingTurnForActiveSession((current) => ({
              ...current,
              blocks,
            }))
          } else if (data.kind === "tool_call_detected") {
            const existingIdx = findToolBlockIndex(blocks, data.invocation_id)
            if (existingIdx >= 0) {
              const b = blocks[existingIdx] as Extract<
                (typeof blocks)[number],
                { type: "tool" }
              >
              blocks[existingIdx] = {
                ...b,
                tool: {
                  ...b.tool,
                  toolName: data.tool_name || b.tool.toolName,
                  arguments: normalizeToolArguments(data.arguments),
                },
              }
            } else {
              blocks.push({
                type: "tool",
                tool: {
                  invocationId: data.invocation_id,
                  toolName: data.tool_name,
                  arguments: normalizeToolArguments(data.arguments),
                  detectedAtMs: Date.now(),
                  output: "",
                  completed: false,
                },
              })
            }
            setStreamingTurnForActiveSession((current) => ({
              ...current,
              blocks,
            }))
          } else if (data.kind === "tool_call_started") {
            const existingIdx = findToolBlockIndex(blocks, data.invocation_id)
            if (existingIdx >= 0) {
              const b = blocks[existingIdx] as Extract<
                (typeof blocks)[number],
                { type: "tool" }
              >
              blocks[existingIdx] = {
                ...b,
                tool: {
                  ...b.tool,
                  toolName: data.tool_name || b.tool.toolName,
                  arguments: normalizeToolArguments(data.arguments),
                  startedAtMs: b.tool.startedAtMs ?? Date.now(),
                },
              }
            } else {
              const startedAtMs = Date.now()
              blocks.push({
                type: "tool",
                tool: {
                  invocationId: data.invocation_id,
                  toolName: data.tool_name,
                  arguments: normalizeToolArguments(data.arguments),
                  detectedAtMs: startedAtMs,
                  startedAtMs,
                  output: "",
                  completed: false,
                },
              })
            }
            setStreamingTurnForActiveSession((current) => ({
              ...current,
              blocks,
            }))
          } else if (data.kind === "tool_output_delta") {
            const idx = findToolBlockIndex(blocks, data.invocation_id)
            if (idx >= 0) {
              const b = blocks[idx] as Extract<
                (typeof blocks)[number],
                { type: "tool" }
              >
              blocks[idx] = {
                ...b,
                tool: {
                  ...b.tool,
                  startedAtMs: b.tool.startedAtMs ?? Date.now(),
                  output: b.tool.output + data.text,
                },
              }
            } else {
              const startedAtMs = Date.now()
              blocks.push({
                type: "tool",
                tool: {
                  invocationId: data.invocation_id,
                  toolName: "",
                  arguments: {},
                  detectedAtMs: startedAtMs,
                  startedAtMs,
                  output: data.text,
                  completed: false,
                },
              })
            }
            setStreamingTurnForActiveSession((current) => ({
              ...current,
              blocks,
            }))
          } else if (data.kind === "tool_call_completed") {
            const idx = findToolBlockIndex(blocks, data.invocation_id)
            if (idx >= 0) {
              const b = blocks[idx] as Extract<
                (typeof blocks)[number],
                { type: "tool" }
              >
              blocks[idx] = {
                ...b,
                tool: {
                  ...b.tool,
                  finishedAtMs: Date.now(),
                  completed: true,
                  resultContent: data.content,
                  resultDetails: data.details,
                  failed: data.failed,
                },
              }
              setStreamingTurnForActiveSession((current) => ({
                ...current,
                blocks,
              }))
            }
          }
          break
        }
        case "turn_completed": {
          if (event.data.session_id !== activeId) break

          set((state) => {
            const turns = [...state.turns, event.data]
            const snapshot: SessionSnapshot = {
              latestTurn: event.data,
              streamingTurn: null,
              chatState: "idle",
              contextPressure: state.contextPressure,
              lastCompression: null,
            }
            return {
              turns,
              streamingTurn: null,
              chatState: "idle" as const,
              error: null,
              lastCompression: null,
              _sessionSnapshots: upsertSessionSnapshot(
                state._sessionSnapshots,
                activeId,
                snapshot,
                state.sessions
              ),
            }
          })
          fetchSessionInfo(activeId ?? undefined)
            .then((info) =>
              set((state) => {
                const snapshot = state._sessionSnapshots[activeId ?? ""]
                if (!activeId || !snapshot) {
                  return { contextPressure: info.pressure_ratio }
                }
                return {
                  contextPressure: info.pressure_ratio,
                  _sessionSnapshots: upsertSessionSnapshot(
                    state._sessionSnapshots,
                    activeId,
                    {
                      ...snapshot,
                      contextPressure: info.pressure_ratio,
                    },
                    state.sessions
                  ),
                }
              })
            )
            .catch(() => {})
          break
        }
        case "context_compressed": {
          if (event.data.session_id !== activeId) break
          set((state) => {
            const snapshot = state._sessionSnapshots[activeId ?? ""]
            return {
              lastCompression: event.data,
              _sessionSnapshots:
                activeId && snapshot
                  ? upsertSessionSnapshot(
                      state._sessionSnapshots,
                      activeId,
                      {
                        ...snapshot,
                        lastCompression: event.data,
                      },
                      state.sessions
                    )
                  : state._sessionSnapshots,
            }
          })
          fetchSessionInfo(activeId ?? undefined)
            .then((info) =>
              set((state) => {
                const snapshot = state._sessionSnapshots[activeId ?? ""]
                if (!activeId || !snapshot) {
                  return { contextPressure: info.pressure_ratio }
                }
                return {
                  contextPressure: info.pressure_ratio,
                  _sessionSnapshots: upsertSessionSnapshot(
                    state._sessionSnapshots,
                    activeId,
                    {
                      ...snapshot,
                      contextPressure: info.pressure_ratio,
                    },
                    state.sessions
                  ),
                }
              })
            )
            .catch(() => {})
          break
        }
        case "error": {
          if (event.data.session_id !== activeId) break
          const latestTurn = get().turns[get().turns.length - 1]
          if (
            !get().streamingTurn &&
            latestTurn?.failure_message === event.data.message
          ) {
            break
          }
          const streamingTurn = get().streamingTurn
          const isCancelledError = event.data.message.includes("已取消")
          if (streamingTurn && isCancelledError) {
            set((state) => {
              const nextStreamingTurn = {
                ...streamingTurn,
                status: "cancelled" as const,
              }
              const snapshot = state._sessionSnapshots[activeId ?? ""]
              return {
                streamingTurn: nextStreamingTurn,
                chatState: "idle",
                error: null,
                _sessionSnapshots:
                  activeId && snapshot
                    ? upsertSessionSnapshot(
                        state._sessionSnapshots,
                        activeId,
                        {
                          ...snapshot,
                          streamingTurn: nextStreamingTurn,
                          chatState: "idle",
                        },
                        state.sessions
                      )
                    : state._sessionSnapshots,
              }
            })
            break
          }
          set((state) => {
            const snapshot = state._sessionSnapshots[activeId ?? ""]
            return {
              error: event.data.message,
              streamingTurn: null,
              chatState: "idle",
              _sessionSnapshots:
                activeId && snapshot
                  ? upsertSessionSnapshot(
                      state._sessionSnapshots,
                      activeId,
                      {
                        ...snapshot,
                        streamingTurn: null,
                        chatState: "idle",
                      },
                      state.sessions
                    )
                  : state._sessionSnapshots,
            }
          })
          break
        }
        case "turn_cancelled": {
          if (event.data.session_id !== activeId) break
          const prev = get().streamingTurn
          if (!prev) break
          set((state) => {
            const nextStreamingTurn = { ...prev, status: "cancelled" as const }
            const snapshot = state._sessionSnapshots[activeId ?? ""]
            return {
              streamingTurn: nextStreamingTurn,
              chatState: "idle",
              error: null,
              _sessionSnapshots:
                activeId && snapshot
                  ? upsertSessionSnapshot(
                      state._sessionSnapshots,
                      activeId,
                      {
                        ...snapshot,
                        streamingTurn: nextStreamingTurn,
                        chatState: "idle",
                      },
                      state.sessions
                    )
                  : state._sessionSnapshots,
            }
          })
          break
        }
        case "session_created": {
          get().fetchSessions()
          break
        }
        case "session_deleted": {
          const deletedId = event.data.session_id
          set((state) => {
            const sessions = state.sessions.filter((s) => s.id !== deletedId)
            const nextSnapshots = trimSessionSnapshotsToKnownSessions(
              state._sessionSnapshots,
              sessions
            )
            if (state.activeSessionId === deletedId) {
              const newActive = sessions[0]?.id ?? null
              return {
                sessions,
                activeSessionId: newActive,
                _sessionSnapshots: nextSnapshots,
              }
            }
            return { sessions, _sessionSnapshots: nextSnapshots }
          })
          break
        }
      }
    },

    submitTurn: (prompt: string) => {
      if (get().chatState === "active") return
      const sessionId = get().activeSessionId
      if (!sessionId) return

      set((state) => {
        const nextStreamingTurn: StreamingTurn = {
          userMessage: prompt,
          status: "waiting",
          blocks: [],
        }
        const snapshot = state._sessionSnapshots[sessionId] ?? EMPTY_SESSION_SNAPSHOT
        const nextState = {
          ...state,
          error: null,
          _pendingPrompt: prompt,
          chatState: "active" as const,
          lastCompression: null,
          streamingTurn: nextStreamingTurn,
        }
        return {
          error: null,
          _pendingPrompt: prompt,
          chatState: "active",
          lastCompression: null,
          streamingTurn: nextStreamingTurn,
          _sessionSnapshots: upsertSessionSnapshot(
            state._sessionSnapshots,
            sessionId,
            {
              ...snapshotFromState(nextState),
              latestTurn: snapshot.latestTurn,
            },
            state.sessions
          ),
        }
      })
      apiSubmitTurn(prompt, sessionId).catch((err: unknown) => {
        set((state) => {
          const nextState = {
            ...state,
            error: err instanceof Error ? err.message : "Network error",
            _pendingPrompt: null,
            streamingTurn: null,
            chatState: "idle" as const,
          }
          return {
            error: err instanceof Error ? err.message : "Network error",
            _pendingPrompt: null,
            streamingTurn: null,
            chatState: "idle",
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              sessionId,
              snapshotFromState(nextState),
              state.sessions
            ),
          }
        })
      })
    },

    cancelTurn: async () => {
      const sessionId = get().activeSessionId
      const streamingTurn = get().streamingTurn
      if (!sessionId || !streamingTurn) return

      try {
        const cancelled = await apiCancelTurn(sessionId)
        if (!cancelled) return
        set((state) => {
          const nextStreamingTurn = {
            ...streamingTurn,
            status: "cancelled" as const,
          }
          const nextState = {
            ...state,
            streamingTurn: nextStreamingTurn,
            chatState: "idle" as const,
            error: null,
          }
          return {
            streamingTurn: nextStreamingTurn,
            chatState: "idle",
            error: null,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              sessionId,
              snapshotFromState(nextState),
              state.sessions
            ),
          }
        })
      } catch (err) {
        set({
          error: err instanceof Error ? err.message : "Cancel failed",
        })
      }
    },

    switchModel: (providerName: string, modelId?: string) => {
      apiSwitchProvider(providerName, modelId)
        .then((info) => {
          set({ provider: info })
          get().refreshProviders()
        })
        .catch((err: unknown) => {
          set({
            error: err instanceof Error ? err.message : "Switch failed",
          })
        })
    },

    refreshProviders: () => {
      Promise.all([apiListProviders(), fetchProviders()])
        .then(([providerList, provider]) => set({ providerList, provider }))
        .catch(() => {})
    },

    setView: (view: AppView) => set({ view }),

    createProvider: async (body) => {
      await apiCreateProvider(body)
      get().refreshProviders()
    },

    updateProvider: async (name, body) => {
      await apiUpdateProvider(name, body)
      get().refreshProviders()
    },

    deleteProvider: async (name) => {
      await apiDeleteProvider(name)
      get().refreshProviders()
    },

    fetchSessions: async () => {
      const sessions = await apiFetchSessions()
      set((state) => ({
        sessions,
        _sessionSnapshots: trimSessionSnapshotsToKnownSessions(
          state._sessionSnapshots,
          sessions
        ),
      }))
    },

    createSession: async () => {
      const session = await apiCreateSession()
      set((state) => ({
        sessions: [...state.sessions, session],
        _sessionSnapshots: trimSessionSnapshotsToKnownSessions(
          state._sessionSnapshots,
          [...state.sessions, session]
        ),
      }))
      await get().switchSession(session.id)
    },

    switchSession: async (id: string) => {
      if (id === get().activeSessionId && !get().sessionHydrating) {
        return
      }
      cancelPendingHistoryHydration()
      await hydrateSession(id)
    },

    loadOlderTurns: async () => {
      const sessionId = get().activeSessionId
      const beforeTurnId = get().historyNextBeforeTurnId
      if (!sessionId || !beforeTurnId || get().historyLoadingMore) return

      set({ historyLoadingMore: true })
      try {
        const historyPage = await fetchHistory({
          sessionId,
          beforeTurnId,
          limit: SESSION_HISTORY_PAGE_SIZE,
        })
        set((state) => {
          const turns = [...historyPage.turns, ...state.turns]
          return {
            turns,
            historyHasMore: historyPage.has_more,
            historyNextBeforeTurnId: historyPage.next_before_turn_id,
            historyLoadingMore: false,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              sessionId,
              {
                ...(state._sessionSnapshots[sessionId] ?? EMPTY_SESSION_SNAPSHOT),
                latestTurn: latestTurn(turns),
              },
              state.sessions
            ),
          }
        })
      } catch {
        set({ historyLoadingMore: false })
      }
    },

    deleteSession: async (id: string) => {
      cancelPendingHistoryHydration()
      await apiDeleteSession(id)
      const state = get()
      const remaining = state.sessions.filter((s) => s.id !== id)
      const nextSnapshots = { ...state._sessionSnapshots }
      delete nextSnapshots[id]
      set({ sessions: remaining, _sessionSnapshots: nextSnapshots })

      if (state.activeSessionId === id) {
        const next = remaining[0]
        if (next) {
          await get().switchSession(next.id)
        } else {
          set({
            activeSessionId: null,
            sessionHydrating: false,
            turns: [],
            historyHasMore: false,
            historyNextBeforeTurnId: null,
            historyLoadingMore: false,
            streamingTurn: null,
            chatState: "idle",
            lastCompression: null,
            contextPressure: null,
          })
        }
      }
    },
  }
})
