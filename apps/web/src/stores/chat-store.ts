import { create } from "zustand"
import {
  connectEvents,
  fetchHistory,
  fetchProviders,
  listProviders as apiListProviders,
  switchProvider as apiSwitchProvider,
  submitTurn as apiSubmitTurn,
  createProvider as apiCreateProvider,
  updateProvider as apiUpdateProvider,
  deleteProvider as apiDeleteProvider,
} from "@/lib/api"
import type {
  AppView,
  ChatState,
  ModelConfig,
  ProviderInfo,
  ProviderListItem,
  SseEvent,
  StreamingTurn,
  TurnLifecycle,
  TurnStatus,
} from "@/lib/types"

type ChatStore = {
  turns: TurnLifecycle[]
  streamingTurn: StreamingTurn | null
  chatState: ChatState
  provider: ProviderInfo | null
  providerList: ProviderListItem[]
  error: string | null
  view: AppView

  // Internal ref for pending prompt
  _pendingPrompt: string | null

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
    },
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
  _pendingPrompt: null,

  initialize: () => {
    fetchProviders()
      .then((provider) => set({ provider }))
      .catch(() => {})
    fetchHistory()
      .then((turns) => set({ turns }))
      .catch(() => {})
    get().refreshProviders()
  },

  handleSseEvent: (event: SseEvent) => {
    switch (event.type) {
      case "status": {
        const status = event.data.status as TurnStatus
        if (status === "waiting") {
          const prompt = get()._pendingPrompt ?? ""
          set({
            _pendingPrompt: null,
            chatState: "active",
            streamingTurn: {
              userMessage: prompt,
              thinkingText: "",
              assistantText: "",
              status: "waiting",
              toolOutputs: [],
            },
          })
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

        if (data.kind === "thinking_delta") {
          set({
            streamingTurn: {
              ...prev,
              thinkingText: prev.thinkingText + data.text,
            },
          })
        } else if (data.kind === "text_delta") {
          set({
            streamingTurn: {
              ...prev,
              assistantText: prev.assistantText + data.text,
            },
          })
        } else if (data.kind === "tool_call_started") {
          const outputs = [...prev.toolOutputs]
          outputs.push({
            invocationId: data.invocation_id,
            toolName: data.tool_name,
            arguments: data.arguments,
            output: "",
          })
          set({ streamingTurn: { ...prev, toolOutputs: outputs } })
        } else if (data.kind === "tool_output_delta") {
          const outputs = [...prev.toolOutputs]
          const idx = outputs.findIndex(
            (t) => t.invocationId === data.invocation_id,
          )
          if (idx >= 0) {
            outputs[idx] = {
              ...outputs[idx],
              output: outputs[idx].output + data.text,
            }
          } else {
            outputs.push({
              invocationId: data.invocation_id,
              toolName: "",
              arguments: {},
              output: data.text,
            })
          }
          set({ streamingTurn: { ...prev, toolOutputs: outputs } })
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
        break
      }
      case "error": {
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
    set({ error: null, _pendingPrompt: prompt })
    apiSubmitTurn(prompt).catch((err: unknown) => {
      set({
        error: err instanceof Error ? err.message : "Network error",
        _pendingPrompt: null,
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
    apiListProviders()
      .then((providerList) => set({ providerList }))
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
