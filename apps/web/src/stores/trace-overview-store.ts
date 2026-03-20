import { create } from "zustand"

import { fetchTraceDashboard } from "@/lib/api"
import type { TraceDashboard, TraceDashboardRange } from "@/lib/types"

type TraceOverviewStore = {
  dashboard: TraceDashboard | null
  range: TraceDashboardRange
  loading: boolean
  initialized: boolean
  error: string | null
  initialize: () => Promise<void>
  refresh: (range?: TraceDashboardRange) => Promise<void>
  setRange: (range: TraceDashboardRange) => Promise<void>
}

export const useTraceOverviewStore = create<TraceOverviewStore>((set, get) => ({
  dashboard: null,
  range: "month",
  loading: false,
  initialized: false,
  error: null,

  initialize: async () => {
    if (get().initialized || get().loading) return
    await get().refresh(get().range)
  },

  refresh: async (nextRange) => {
    if (get().loading) return

    const range = nextRange ?? get().range
    set({ loading: true, error: null, range })

    try {
      const dashboard = await fetchTraceDashboard({ range })
      set({
        dashboard,
        loading: false,
        initialized: true,
        range,
      })
    } catch (error) {
      set({
        loading: false,
        error:
          error instanceof Error
            ? error.message
            : "Failed to load trace overview",
      })
      throw error
    }
  },

  setRange: async (range) => {
    if (get().range === range && get().dashboard) return
    await get().refresh(range)
  },
}))
