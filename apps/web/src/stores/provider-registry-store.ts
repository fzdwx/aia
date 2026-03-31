import { create } from "zustand"

import {
  createProvider as apiCreateProvider,
  deleteProvider as apiDeleteProvider,
  listProviders as apiListProviders,
  updateProvider as apiUpdateProvider,
} from "@/lib/api"
import type {
  ModelConfig,
  ProviderCredentialInput,
  ProviderListItem,
} from "@/lib/types"

type ProviderRegistryStore = {
  providerList: ProviderListItem[]
  refreshProviders: () => Promise<void>
  createProvider: (body: {
    id: string
    label: string
    adapter: string
    credential: ProviderCredentialInput
    models: ModelConfig[]
    base_url: string
  }) => Promise<void>
  updateProvider: (
    id: string,
    body: {
      label?: string
      adapter?: string
      credential?: ProviderCredentialInput
      models?: ModelConfig[]
      base_url?: string
    }
  ) => Promise<void>
  deleteProvider: (id: string) => Promise<void>
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
    updateProvider: async (id, body) => {
      await apiUpdateProvider(id, body)
      await useProviderRegistryStore.getState().refreshProviders()
    },
    deleteProvider: async (id) => {
      await apiDeleteProvider(id)
      await useProviderRegistryStore.getState().refreshProviders()
    },
  })
)
