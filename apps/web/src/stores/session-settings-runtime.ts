import type { ProviderInfo, ThinkingLevel } from "@/lib/types"
import { useChatStore } from "@/stores/chat-store"
import { useProviderRegistryStore } from "@/stores/provider-registry-store"
import {
  setSessionReasoningThroughCoordinator,
  switchSessionModelThroughCoordinator,
} from "@/stores/session-settings-coordinator"

function syncActiveSessionModelProjection(
  sessionId: string,
  modelId: string
): void {
  useChatStore.setState((state) => ({
    sessions: state.sessions.map((session) =>
      session.id === sessionId ? { ...session, model: modelId } : session
    ),
  }))
}

export async function switchActiveSessionModel(
  providerName: string,
  modelId: string,
  reasoningEffort?: ThinkingLevel | null
): Promise<ProviderInfo | null> {
  const sessionId = useChatStore.getState().activeSessionId
  if (!sessionId) return null

  const info = await switchSessionModelThroughCoordinator(
    sessionId,
    providerName,
    modelId,
    reasoningEffort
  )
  syncActiveSessionModelProjection(sessionId, modelId)
  await useProviderRegistryStore.getState().refreshProviders()
  return info
}

export async function setActiveSessionReasoningEffort(
  reasoningEffort: ThinkingLevel | null
): Promise<ProviderInfo | null> {
  const sessionId = useChatStore.getState().activeSessionId
  if (!sessionId) return null

  const info = await setSessionReasoningThroughCoordinator(
    sessionId,
    reasoningEffort
  )
  if (info) {
    await useProviderRegistryStore.getState().refreshProviders()
  }
  return info
}
