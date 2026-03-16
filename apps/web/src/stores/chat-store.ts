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

type ChatStore = {
  // Session management
  sessions: SessionListItem[]
  activeSessionId: string | null

  // Per-session state
  turns: TurnLifecycle[]
  streamingTurn: StreamingTurn | null
  chatState: ChatState
  provider: ProviderInfo | null
  providerList: ProviderListItem[]
  error: string | null
  view: AppView
  contextPressure: number | null
  lastCompression: ContextCompressionNotice | null

  // Internal ref for pending prompt
  _pendingPrompt: string | null

  _refreshProviderInfo: () => Promise<void>

  // Actions
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

  // Session actions
  fetchSessions: () => Promise<void>
  createSession: () => Promise<void>
  switchSession: (id: string) => Promise<void>
  deleteSession: (id: string) => Promise<void>
}

export const useChatStore = create<ChatStore>((set, get) => ({
  sessions: [],
  activeSessionId: null,
  turns: [],
  streamingTurn: null,
  chatState: "idle",
  provider: null,
  providerList: [],
  error: null,
  view: "chat",
  contextPressure: null,
  lastCompression: null,
  _pendingPrompt: null,

  _refreshProviderInfo: async () => {
    const provider = await fetchProviders()
    set({ provider })
  },

  initialize: () => {
    get()
      ._refreshProviderInfo()
      .catch(() => {})

    // Load sessions first, then load data for the first session
    apiFetchSessions()
      .then((sessions) => {
        const activeId = sessions[0]?.id ?? null
        set({ sessions, activeSessionId: activeId })
        if (activeId) {
          Promise.all([fetchHistory(activeId), fetchCurrentTurn(activeId)])
            .then(([turns, currentTurn]) =>
              set({
                turns,
                streamingTurn: currentTurn
                  ? currentTurnToStreamingTurn(currentTurn)
                  : null,
                chatState: currentTurn ? "active" : "idle",
                lastCompression: null,
              })
            )
            .catch(() => {})
          fetchSessionInfo(activeId)
            .then((info) => set({ contextPressure: info.pressure_ratio }))
            .catch(() => {})
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
        // Filter by active session
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
        // Filter by active session
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
        // Filter by active session
        if (event.data.session_id !== activeId) break

        set((state) => ({
          turns: [...state.turns, event.data],
          streamingTurn: null,
          chatState: "idle" as const,
          error: null,
          lastCompression: null,
        }))
        fetchSessionInfo(activeId ?? undefined)
          .then((info) => set({ contextPressure: info.pressure_ratio }))
          .catch(() => {})
        break
      }
      case "context_compressed": {
        if (event.data.session_id !== activeId) break
        set({ lastCompression: event.data })
        fetchSessionInfo(activeId ?? undefined)
          .then((info) => set({ contextPressure: info.pressure_ratio }))
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
          set({
            streamingTurn: { ...streamingTurn, status: "cancelled" },
            chatState: "idle",
            error: null,
          })
          break
        }
        set({
          error: event.data.message,
          streamingTurn: null,
          chatState: "idle",
        })
        break
      }
      case "turn_cancelled": {
        if (event.data.session_id !== activeId) break
        const prev = get().streamingTurn
        if (!prev) break
        set({
          streamingTurn: { ...prev, status: "cancelled" },
          chatState: "idle",
          error: null,
        })
        break
      }
      case "session_created": {
        // Add to session list
        get().fetchSessions()
        break
      }
      case "session_deleted": {
        const deletedId = event.data.session_id
        set((state) => {
          const sessions = state.sessions.filter((s) => s.id !== deletedId)
          // If the deleted session was active, switch to first available
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

        set({
          error: null,
          _pendingPrompt: prompt,
          chatState: "active",
          lastCompression: null,
          streamingTurn: {
            userMessage: prompt,
            status: "waiting",
            blocks: [],
          },
        })
    apiSubmitTurn(prompt, sessionId).catch((err: unknown) => {
      set({
        error: err instanceof Error ? err.message : "Network error",
        _pendingPrompt: null,
        streamingTurn: null,
        chatState: "idle",
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
      set({
        streamingTurn: { ...streamingTurn, status: "cancelled" },
        chatState: "idle",
        error: null,
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

  // ── Session actions ────────────────────────────────────────

  fetchSessions: async () => {
    const sessions = await apiFetchSessions()
    set({ sessions })
  },

  createSession: async () => {
    const session = await apiCreateSession()
    set((state) => ({
      sessions: [...state.sessions, session],
    }))
    // Auto-switch to the new session
    await get().switchSession(session.id)
  },

  switchSession: async (id: string) => {
    set({
      activeSessionId: id,
      turns: [],
      streamingTurn: null,
      chatState: "idle",
      error: null,
      contextPressure: null,
      lastCompression: null,
    })

    try {
      const [turns, currentTurn] = await Promise.all([
        fetchHistory(id),
        fetchCurrentTurn(id),
      ])
      set({
        turns,
        streamingTurn: currentTurn
          ? currentTurnToStreamingTurn(currentTurn)
          : null,
        chatState: currentTurn ? "active" : "idle",
      })
    } catch {
      // ignore
    }

    fetchSessionInfo(id)
      .then((info) => set({ contextPressure: info.pressure_ratio }))
      .catch(() => {})
  },

  deleteSession: async (id: string) => {
    await apiDeleteSession(id)
    const state = get()
    const remaining = state.sessions.filter((s) => s.id !== id)
    set({ sessions: remaining })

    // If we deleted the active session, switch to first remaining
    if (state.activeSessionId === id) {
      const next = remaining[0]
      if (next) {
        await get().switchSession(next.id)
      } else {
        set({
          activeSessionId: null,
          turns: [],
          streamingTurn: null,
          chatState: "idle",
          lastCompression: null,
        })
      }
    }
  },
}))
