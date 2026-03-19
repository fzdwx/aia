import { create } from "zustand"

import {
  createChannel as apiCreateChannel,
  deleteChannel as apiDeleteChannel,
  listChannels as apiListChannels,
  listSupportedChannels as apiListSupportedChannels,
  updateChannel as apiUpdateChannel,
} from "@/lib/api"
import type {
  ChannelListItem,
  ChannelTransport,
  CreateChannelRequest,
  SupportedChannelDefinition,
  UpdateChannelRequest,
} from "@/lib/types"

type ChannelsStore = {
  supportedChannels: SupportedChannelDefinition[]
  configuredChannels: ChannelListItem[]
  selectedTransport: ChannelTransport | null
  loading: boolean
  initialized: boolean
  error: string | null
  initialize: () => Promise<void>
  refresh: () => Promise<void>
  selectTransport: (transport: ChannelTransport) => void
  createChannel: (body: CreateChannelRequest) => Promise<void>
  updateChannel: (id: string, body: UpdateChannelRequest) => Promise<void>
  deleteChannel: (id: string) => Promise<void>
}

function defaultSelectedTransport(
  supportedChannels: SupportedChannelDefinition[],
  current: ChannelTransport | null
): ChannelTransport | null {
  if (
    current &&
    supportedChannels.some((channel) => channel.transport === current)
  ) {
    return current
  }
  return supportedChannels[0]?.transport ?? null
}

export const useChannelsStore = create<ChannelsStore>((set, get) => ({
  supportedChannels: [],
  configuredChannels: [],
  selectedTransport: null,
  loading: false,
  initialized: false,
  error: null,

  initialize: async () => {
    if (get().initialized || get().loading) return
    await get().refresh()
  },

  refresh: async () => {
    if (get().loading) return

    set({ loading: true, error: null })

    try {
      const [supportedChannels, configuredChannels] = await Promise.all([
        apiListSupportedChannels(),
        apiListChannels(),
      ])

      set((state) => ({
        supportedChannels,
        configuredChannels,
        selectedTransport: defaultSelectedTransport(
          supportedChannels,
          state.selectedTransport
        ),
        loading: false,
        initialized: true,
      }))
    } catch (error) {
      set({
        loading: false,
        error:
          error instanceof Error ? error.message : "Failed to load channels",
      })
      throw error
    }
  },

  selectTransport: (transport) => set({ selectedTransport: transport }),

  createChannel: async (body) => {
    await apiCreateChannel(body)
    await get().refresh()
  },

  updateChannel: async (id, body) => {
    await apiUpdateChannel(id, body)
    await get().refresh()
  },

  deleteChannel: async (id) => {
    await apiDeleteChannel(id)
    await get().refresh()
  },
}))
