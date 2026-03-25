import { create } from "zustand"

import type { AppView, ProviderListItem } from "@/lib/types"
import { NEW_PROVIDER_SETTINGS_KEY } from "@/stores/chat-store"

export type SettingsSection = "providers" | "channels"

function resolveSelectedProviderName(
  providerList: ProviderListItem[],
  current: string | null
): string | null {
  if (current === NEW_PROVIDER_SETTINGS_KEY) {
    return current
  }

  if (current && providerList.some((provider) => provider.name === current)) {
    return current
  }

  return (
    providerList.find((provider) => provider.active)?.name ??
    providerList[0]?.name ??
    null
  )
}

type WorkbenchStore = {
  view: AppView
  settingsSection: SettingsSection
  selectedProviderName: string | null
  setView: (view: AppView) => void
  setSettingsSection: (section: SettingsSection) => void
  selectProviderName: (name: string | null) => void
  reconcileProviderSelection: (providerList: ProviderListItem[]) => void
}

export const useWorkbenchStore = create<WorkbenchStore>((set) => ({
  view: "chat",
  settingsSection: "providers",
  selectedProviderName: null,
  setView: (view) =>
    set((state) => ({
      view: view === "channels" ? "settings" : view,
      settingsSection:
        view === "channels"
          ? "channels"
          : view === "settings"
            ? state.settingsSection
            : state.settingsSection,
    })),
  setSettingsSection: (settingsSection) =>
    set({
      settingsSection,
      view: "settings",
    }),
  selectProviderName: (selectedProviderName) =>
    set({
      selectedProviderName,
      settingsSection: "providers",
      view: "settings",
    }),
  reconcileProviderSelection: (providerList) =>
    set((state) => ({
      selectedProviderName: resolveSelectedProviderName(
        providerList,
        state.selectedProviderName
      ),
    })),
}))
