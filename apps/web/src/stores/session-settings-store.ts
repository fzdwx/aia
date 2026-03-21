import { create } from "zustand"

import {
  fetchSessionSettings,
  updateSessionSettings as apiUpdateSessionSettings,
} from "@/lib/api"
import type {
  ProviderInfo,
  ProviderListItem,
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
  sessionSettings: SessionSettings | null
  hydrating: boolean
  updating: boolean
  error: string | null
  hydrateForSession: (sessionId: string) => Promise<void>
  clear: () => void
  supportsReasoning: (providerList: ProviderListItem[]) => boolean
  switchModel: (
    sessionId: string,
    providerList: ProviderListItem[],
    providerName: string,
    modelId: string,
    reasoningEffort?: ThinkingLevel | null
  ) => Promise<ProviderInfo>
  setReasoningEffort: (
    sessionId: string,
    providerList: ProviderListItem[],
    reasoningEffort: ThinkingLevel | null
  ) => Promise<ProviderInfo | null>
}

let latestHydrationRequestId = 0
let latestMutationRequestId = 0

export const useSessionSettingsStore = create<SessionSettingsStore>(
  (set, get) => ({
    sessionSettings: null,
    hydrating: false,
    updating: false,
    error: null,

    hydrateForSession: async (sessionId: string) => {
      const requestId = ++latestHydrationRequestId
      latestMutationRequestId += 1
      set({ hydrating: true, updating: false, error: null })
      try {
        const sessionSettings = await fetchSessionSettings(sessionId)
        if (requestId !== latestHydrationRequestId) return
        set({ sessionSettings, hydrating: false })
      } catch (error) {
        if (requestId !== latestHydrationRequestId) return
        set({
          hydrating: false,
          error:
            error instanceof Error
              ? error.message
              : "Failed to load session settings",
        })
      }
    },

    clear: () =>
      set(() => {
        latestHydrationRequestId += 1
        latestMutationRequestId += 1
        return {
          sessionSettings: null,
          hydrating: false,
          updating: false,
          error: null,
        }
      }),

    supportsReasoning: (providerList) => {
      const match = findProviderModel(providerList, get().sessionSettings)
      return match?.model.supports_reasoning === true
    },

    switchModel: async (
      sessionId,
      providerList,
      providerName,
      modelId,
      reasoningEffort
    ) => {
      const provider = providerList.find((item) => item.name === providerName)
      if (!provider) {
        throw new Error(`provider not found: ${providerName}`)
      }
      const model = provider.models.find((item) => item.id === modelId)
      if (!model) {
        throw new Error(`model not found: ${modelId}`)
      }

      const nextReasoningEffort = model.supports_reasoning
        ? (reasoningEffort ?? get().sessionSettings?.reasoning_effort ?? null)
        : null

      const requestId = ++latestMutationRequestId
      set({ updating: true, error: null })
      try {
        const info = await apiUpdateSessionSettings({
          session_id: sessionId,
          provider: providerName,
          model: modelId,
          reasoning_effort: nextReasoningEffort,
        })

        if (requestId !== latestMutationRequestId) {
          return info
        }

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
        if (requestId !== latestMutationRequestId) {
          throw error
        }
        set({
          updating: false,
          error:
            error instanceof Error
              ? error.message
              : "Failed to update session settings",
        })
        throw error
      }
    },

    setReasoningEffort: async (sessionId, providerList, reasoningEffort) => {
      const sessionSettings = get().sessionSettings
      if (!sessionSettings) return null
      return get().switchModel(
        sessionId,
        providerList,
        sessionSettings.provider,
        sessionSettings.model,
        reasoningEffort
      )
    },
  })
)
