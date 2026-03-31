import type { SessionSelectionInfo, ThinkingLevel } from "@/lib/types"
import { useProviderRegistryStore } from "@/stores/provider-registry-store"
import { useSessionSettingsStore } from "@/stores/session-settings-store"

export async function hydrateSessionSettingsForSession(
  sessionId: string
): Promise<void> {
  await useSessionSettingsStore.getState().hydrateForSession(sessionId)
}

export function clearSessionSettingsState(): void {
  useSessionSettingsStore.getState().clear()
}

export async function switchSessionModelThroughCoordinator(
  sessionId: string,
  providerId: string,
  modelId: string,
  reasoningEffort?: ThinkingLevel | null
): Promise<SessionSelectionInfo> {
  return useSessionSettingsStore
    .getState()
    .switchModel(
      sessionId,
      useProviderRegistryStore.getState().providerList,
      providerId,
      modelId,
      reasoningEffort
    )
}

export async function setSessionReasoningThroughCoordinator(
  sessionId: string,
  reasoningEffort: ThinkingLevel | null
): Promise<SessionSelectionInfo | null> {
  return useSessionSettingsStore
    .getState()
    .setReasoningEffort(
      sessionId,
      useProviderRegistryStore.getState().providerList,
      reasoningEffort
    )
}
