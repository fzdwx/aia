import { create } from "zustand"

import {
  fetchSessionSettings,
  updateSessionSettings as apiUpdateSessionSettings,
} from "@/lib/api"
import type {
  ProviderInfo,
  ProviderListItem,
  SessionListItem,
  SessionSettings,
  ThinkingLevel,
} from "@/lib/types"

function findProviderModel(
  providerList: ProviderListItem[],
  settings: SessionSettings | null
) {
  if (!settings) return null
  const provider = providerList.find((item) => item.name === settings.provider)
  if (!provider) return null
  const model = provider.models.find((item) => item.id === settings.model)
  if (!model) return null
  return { provider, model }
}

type SessionSettingsStore = {
  activeSessionId: string | null
  sessionSettings: SessionSettings | null
  hydrating: boolean
  updating: boolean
  error: string | null
  setActiveSessionId: (sessionId: string | null) => void
  hydrateForSession: (sessionId: string) => Promise<void>
  clear: () => void
  supportsReasoning: (providerList: ProviderListItem[]) => boolean
  switchModel: (
    providerList: ProviderListItem[],
    providerName: string,
    modelId: string,
    reasoningEffort?: ThinkingLevel | null
  ) => Promise<ProviderInfo>
  setReasoningEffort: (
    providerList: ProviderListItem[],
    reasoningEffort: ThinkingLevel | null
  ) => Promise<ProviderInfo | null>
  syncSessionListModel: (
    sessions: SessionListItem[],
    sessionId: string,
    modelId: string
  ) => SessionListItem[]
}

export const useSessionSettingsStore = create<SessionSettingsStore>((set, get) => ({
  activeSessionId: null,
  sessionSettings: null,
  hydrating: false,
  updating: false,
  error: null,

  setActiveSessionId: (activeSessionId) => set({ activeSessionId }),

  hydrateForSession: async (sessionId: string) => {
    set({ activeSessionId: sessionId, hydrating: true, error: null })
    try {
      const sessionSettings = await fetchSessionSettings(sessionId)
      if (get().activeSessionId !== sessionId) return
      set({ sessionSettings, hydrating: false })
    } catch (error) {
      if (get().activeSessionId !== sessionId) return
      set({
        hydrating: false,
        error:
          error instanceof Error ? error.message : "Failed to load session settings",
      })
    }
  },

  clear: () =>
    set({
      activeSessionId: null,
      sessionSettings: null,
      hydrating: false,
      updating: false,
      error: null,
    }),

  supportsReasoning: (providerList) => {
    const match = findProviderModel(providerList, get().sessionSettings)
    return match?.model.supports_reasoning === true
  },

  switchModel: async (providerList, providerName, modelId, reasoningEffort) => {
    const activeSessionId = get().activeSessionId
    if (!activeSessionId) {
      throw new Error("no active session")
    }

    const provider = providerList.find((item) => item.name === providerName)
    if (!provider) {
      throw new Error(`provider not found: ${providerName}`)
    }
    const model = provider.models.find((item) => item.id === modelId)
    if (!model) {
      throw new Error(`model not found: ${modelId}`)
    }

    const nextReasoningEffort = model.supports_reasoning
      ? reasoningEffort ?? model.reasoning_effort ?? null
      : null

    set({ updating: true, error: null })
    try {
      const info = await apiUpdateSessionSettings({
        session_id: activeSessionId,
        provider: providerName,
        model: modelId,
        reasoning_effort: nextReasoningEffort,
      })

      set({
        updating: false,
        sessionSettings: {
          provider: providerName,
          model: modelId,
          protocol: provider.kind,
          reasoning_effort: nextReasoningEffort,
        },
      })

      return info
    } catch (error) {
      set({
        updating: false,
        error:
          error instanceof Error ? error.message : "Failed to update session settings",
      })
      throw error
    }
  },

  setReasoningEffort: async (providerList, reasoningEffort) => {
    const sessionSettings = get().sessionSettings
    if (!sessionSettings) return null
    return get().switchModel(
      providerList,
      sessionSettings.provider,
      sessionSettings.model,
      reasoningEffort
    )
  },

  syncSessionListModel: (sessions, sessionId, modelId) =>
    sessions.map((session) =>
      session.id === sessionId ? { ...session, model: modelId } : session
    ),
}))
