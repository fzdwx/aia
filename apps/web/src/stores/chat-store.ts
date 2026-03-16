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

type SessionSnapshot = {
  turns: TurnLifecycle[]
  historyHasMore: boolean
  historyNextBeforeTurnId: string | null
  streamingTurn: StreamingTurn | null
  chatState: ChatState
  contextPressure: number | null
  lastCompression: ContextCompressionNotice | null
}

const EMPTY_SESSION_SNAPSHOT: SessionSnapshot = {
  turns: [],
  historyHasMore: false,
  historyNextBeforeTurnId: null,
  streamingTurn: null,
  chatState: "idle",
  contextPressure: null,
  lastCompression: null,
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
    turns: snapshot.turns,
    historyHasMore: snapshot.historyHasMore,
    historyNextBeforeTurnId: snapshot.historyNextBeforeTurnId,
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
  snapshot: SessionSnapshot
): Record<string, SessionSnapshot> {
  if (!sessionId) return snapshots
  return {
    ...snapshots,
    [sessionId]: snapshot,
  }
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

export const useChatStore = create<ChatStore>((set, get) => {
  async function hydrateSession(id: string) {
    const loadId = ++latestSessionLoadId
    const cachedSnapshot = get()._sessionSnapshots[id] ?? EMPTY_SESSION_SNAPSHOT

    set((state) => ({
      activeSessionId: id,
      ...applySessionSnapshot(cachedSnapshot, true),
      _sessionSnapshots: upsertSessionSnapshot(
        state._sessionSnapshots,
        state.activeSessionId,
        {
          turns: state.turns,
          historyHasMore: state.historyHasMore,
          historyNextBeforeTurnId: state.historyNextBeforeTurnId,
          streamingTurn: state.streamingTurn,
          chatState: state.chatState,
          contextPressure: state.contextPressure,
          lastCompression: state.lastCompression,
        }
      ),
    }))

    fetchSessionInfo(id)
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
              snapshot
            ),
          }
        })
      })
      .catch(() => {})

    try {
      const [historyPage, currentTurn] = await Promise.all([
        fetchHistory({ sessionId: id, limit: SESSION_HISTORY_PAGE_SIZE }),
        fetchCurrentTurn(id),
      ])

      if (loadId !== latestSessionLoadId) {
        return
      }

      const hydratedSnapshot: SessionSnapshot = {
        turns: historyPage.turns,
        historyHasMore: historyPage.has_more,
        historyNextBeforeTurnId: historyPage.next_before_turn_id,
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
          hydratedSnapshot
        ),
      }))
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

      switch (event.type) {
        case "status": {
          if (event.data.session_id !== activeId) break

          const status = event.data.status as TurnStatus
          if (status === "waiting") {
            const prev = get().streamingTurn
            if (prev) {
              set({
                _pendingPrompt: null,
                chatState: "active",
                streamingTurn: { ...prev, status: "waiting" },
              })
            } else {
              const prompt = get()._pendingPrompt ?? ""
              set({
                _pendingPrompt: null,
                chatState: "active",
                streamingTurn: {
                  userMessage: prompt,
                  status: "waiting",
                  blocks: [],
                },
              })
            }
          } else {
            const prev = get().streamingTurn
            if (prev) {
              set({ streamingTurn: { ...prev, status } })
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
            set({ streamingTurn: { ...prev, blocks } })
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
            set({ streamingTurn: { ...prev, blocks } })
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
            set({ streamingTurn: { ...prev, blocks } })
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
            set({ streamingTurn: { ...prev, blocks } })
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
            set({ streamingTurn: { ...prev, blocks } })
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
              set({ streamingTurn: { ...prev, blocks } })
            }
          }
          break
        }
        case "turn_completed": {
          if (event.data.session_id !== activeId) break

          set((state) => {
            const turns = [...state.turns, event.data]
            const snapshot: SessionSnapshot = {
              turns,
              historyHasMore: state.historyHasMore,
              historyNextBeforeTurnId: state.historyNextBeforeTurnId,
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
                snapshot
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
                    }
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
                  ? upsertSessionSnapshot(state._sessionSnapshots, activeId, {
                      ...snapshot,
                      lastCompression: event.data,
                    })
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
                    }
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
                    ? upsertSessionSnapshot(state._sessionSnapshots, activeId, {
                        ...snapshot,
                        streamingTurn: nextStreamingTurn,
                        chatState: "idle",
                      })
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
                  ? upsertSessionSnapshot(state._sessionSnapshots, activeId, {
                      ...snapshot,
                      streamingTurn: null,
                      chatState: "idle",
                    })
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
                  ? upsertSessionSnapshot(state._sessionSnapshots, activeId, {
                      ...snapshot,
                      streamingTurn: nextStreamingTurn,
                      chatState: "idle",
                    })
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
            if (state.activeSessionId === deletedId) {
              const newActive = sessions[0]?.id ?? null
              return { sessions, activeSessionId: newActive }
            }
            return { sessions }
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
              ...snapshot,
              streamingTurn: nextStreamingTurn,
              chatState: "active",
              lastCompression: null,
            }
          ),
        }
      })
      apiSubmitTurn(prompt, sessionId).catch((err: unknown) => {
        set((state) => ({
          error: err instanceof Error ? err.message : "Network error",
          _pendingPrompt: null,
          streamingTurn: null,
          chatState: "idle",
          _sessionSnapshots: upsertSessionSnapshot(
            state._sessionSnapshots,
            sessionId,
            {
              ...(state._sessionSnapshots[sessionId] ?? EMPTY_SESSION_SNAPSHOT),
              streamingTurn: null,
              chatState: "idle",
            }
          ),
        }))
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
          return {
            streamingTurn: nextStreamingTurn,
            chatState: "idle",
            error: null,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              sessionId,
              {
                ...(state._sessionSnapshots[sessionId] ?? EMPTY_SESSION_SNAPSHOT),
                streamingTurn: nextStreamingTurn,
                chatState: "idle",
              }
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
      set({ sessions })
    },

    createSession: async () => {
      const session = await apiCreateSession()
      set((state) => ({
        sessions: [...state.sessions, session],
      }))
      await get().switchSession(session.id)
    },

    switchSession: async (id: string) => {
      if (id === get().activeSessionId && !get().sessionHydrating) {
        return
      }
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
                turns,
                historyHasMore: historyPage.has_more,
                historyNextBeforeTurnId: historyPage.next_before_turn_id,
              }
            ),
          }
        })
      } catch {
        set({ historyLoadingMore: false })
      }
    },

    deleteSession: async (id: string) => {
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
