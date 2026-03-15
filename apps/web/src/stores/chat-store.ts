import { create } from "zustand"
import {
  fetchCurrentTurn,
  fetchHistory,
  fetchProviders,
  fetchSessionInfo,
  listProviders as apiListProviders,
  switchProvider as apiSwitchProvider,
  submitTurn as apiSubmitTurn,
  createProvider as apiCreateProvider,
  updateProvider as apiUpdateProvider,
  deleteProvider as apiDeleteProvider,
} from "@/lib/api"
import { normalizeToolArguments } from "@/lib/tool-display"
import type {
  AppView,
  ChatState,
  CurrentTurnSnapshot,
  ModelConfig,
  ProviderInfo,
  ProviderListItem,
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
  turns: TurnLifecycle[]
  streamingTurn: StreamingTurn | null
  chatState: ChatState
  provider: ProviderInfo | null
  providerList: ProviderListItem[]
  error: string | null
  view: AppView
  contextPressure: number | null

  // Internal ref for pending prompt
  _pendingPrompt: string | null

  _refreshProviderInfo: () => Promise<void>

  // Actions
  initialize: () => void
  handleSseEvent: (event: SseEvent) => void
  submitTurn: (prompt: string) => void
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
}

export const useChatStore = create<ChatStore>((set, get) => ({
  turns: [],
  streamingTurn: null,
  chatState: "idle",
  provider: null,
  providerList: [],
  error: null,
  view: "chat",
  contextPressure: null,
  _pendingPrompt: null,

  _refreshProviderInfo: async () => {
    const provider = await fetchProviders()
    set({ provider })
  },

  initialize: () => {
    get()
      ._refreshProviderInfo()
      .catch(() => {})
    Promise.all([fetchHistory(), fetchCurrentTurn()])
      .then(([turns, currentTurn]) =>
        set({
          turns,
          streamingTurn: currentTurn
            ? currentTurnToStreamingTurn(currentTurn)
            : null,
          chatState: currentTurn ? "active" : "idle",
        })
      )
      .catch(() => {})
    fetchSessionInfo()
      .then((info) => set({ contextPressure: info.pressure_ratio }))
      .catch(() => {})
    get().refreshProviders()
  },

  handleSseEvent: (event: SseEvent) => {
    switch (event.type) {
      case "status": {
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
        } else if (data.kind === "tool_call_started") {
          let existingIdx = -1
          for (let i = blocks.length - 1; i >= 0; i -= 1) {
            const block = blocks[i]
            if (
              block?.type === "tool" &&
              block.tool.invocationId === data.invocation_id
            ) {
              existingIdx = i
              break
            }
          }
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
          let idx = -1
          for (let i = blocks.length - 1; i >= 0; i -= 1) {
            const block = blocks[i]
            if (
              block?.type === "tool" &&
              block.tool.invocationId === data.invocation_id
            ) {
              idx = i
              break
            }
          }
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
          let idx = -1
          for (let i = blocks.length - 1; i >= 0; i -= 1) {
            const block = blocks[i]
            if (
              block?.type === "tool" &&
              block.tool.invocationId === data.invocation_id
            ) {
              idx = i
              break
            }
          }
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
        set((state) => ({
          turns: [...state.turns, event.data],
          streamingTurn: null,
          chatState: "idle" as const,
          error: null,
        }))
        fetchSessionInfo()
          .then((info) => set({ contextPressure: info.pressure_ratio }))
          .catch(() => {})
        break
      }
      case "context_compressed": {
        fetchSessionInfo()
          .then((info) => set({ contextPressure: info.pressure_ratio }))
          .catch(() => {})
        break
      }
      case "error": {
        const latestTurn = get().turns[get().turns.length - 1]
        if (
          !get().streamingTurn &&
          latestTurn?.failure_message === event.data.message
        ) {
          break
        }
        set({
          error: event.data.message,
          streamingTurn: null,
          chatState: "idle",
        })
        break
      }
    }
  },

  submitTurn: (prompt: string) => {
    if (get().chatState === "active") return
    set({
      error: null,
      _pendingPrompt: prompt,
      chatState: "active",
      streamingTurn: {
        userMessage: prompt,
        status: "waiting",
        blocks: [],
      },
    })
    apiSubmitTurn(prompt).catch((err: unknown) => {
      set({
        error: err instanceof Error ? err.message : "Network error",
        _pendingPrompt: null,
        streamingTurn: null,
        chatState: "idle",
      })
    })
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
}))
