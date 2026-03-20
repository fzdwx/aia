import { create } from "zustand"

import { fetchTraceSummary } from "@/lib/api"
import type { TraceSummary } from "@/lib/types"

type TraceOverviewStore = {
  overallSummary: TraceSummary | null
  conversationSummary: TraceSummary | null
  compressionSummary: TraceSummary | null
  loading: boolean
  initialized: boolean
  error: string | null
  initialize: () => Promise<void>
  refresh: () => Promise<void>
}

export const useTraceOverviewStore = create<TraceOverviewStore>((set, get) => ({
  overallSummary: null,
  conversationSummary: null,
  compressionSummary: null,
  loading: false,
  initialized: false,
  error: null,

  initialize: async () => {
    if (get().initialized || get().loading) return
    await get().refresh()
  },

  refresh: async () => {
    if (get().loading) return

    set({
      loading: true,
      error: null,
    })

    try {
      const [overallSummary, conversationSummary, compressionSummary] =
        await Promise.all([
          fetchTraceSummary(),
          fetchTraceSummary({ request_kind: "completion" }),
          fetchTraceSummary({ request_kind: "compression" }),
        ])

      set({
        overallSummary,
        conversationSummary,
        compressionSummary,
        loading: false,
        initialized: true,
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
}))
