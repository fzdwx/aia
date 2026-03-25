import { create } from "zustand"

import {
  createProvider as apiCreateProvider,
  deleteProvider as apiDeleteProvider,
  listProviders as apiListProviders,
  updateProvider as apiUpdateProvider,
} from "@/lib/api"
import type { ModelConfig, ProviderListItem } from "@/lib/types"

type ProviderRegistryStore = {
  providerList: ProviderListItem[]
  refreshProviders: () => Promise<void>
  createProvider: (body: {
    name: string
    kind: string
    models: ModelConfig[]
    api_key: string
    base_url: string
  }) => Promise<void>
  updateProvider: (
    name: string,
    body: {
      kind?: string
      models?: ModelConfig[]
      api_key?: string
      base_url?: string
    }
  ) => Promise<void>
  deleteProvider: (name: string) => Promise<void>
}

export const useProviderRegistryStore = create<ProviderRegistryStore>(
  (set) => ({
    providerList: [],
    refreshProviders: async () => {
      try {
        const providerList = await apiListProviders()
        set({ providerList })
      } catch {
        return
      }
    },
    createProvider: async (body) => {
      await apiCreateProvider(body)
      await useProviderRegistryStore.getState().refreshProviders()
    },
    updateProvider: async (name, body) => {
      await apiUpdateProvider(name, body)
      await useProviderRegistryStore.getState().refreshProviders()
    },
    deleteProvider: async (name) => {
      await apiDeleteProvider(name)
      await useProviderRegistryStore.getState().refreshProviders()
    },
  })
)
