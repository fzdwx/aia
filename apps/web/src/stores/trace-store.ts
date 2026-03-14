import { create } from "zustand"
import { fetchTrace, fetchTraceSummary, fetchTraces } from "@/lib/api"
import type { TraceListItem, TraceRecord, TraceSummary } from "@/lib/types"

type TraceStore = {
  traces: TraceListItem[]
  selectedTraceId: string | null
  selectedTrace: TraceRecord | null
  traceSummary: TraceSummary | null
  traceLoading: boolean
  traceError: string | null
  refreshTraces: () => Promise<void>
  selectTrace: (traceId: string) => Promise<void>
  clearSelection: () => void
}

export const useTraceStore = create<TraceStore>((set, get) => ({
  traces: [],
  selectedTraceId: null,
  selectedTrace: null,
  traceSummary: null,
  traceLoading: false,
  traceError: null,

  refreshTraces: async () => {
    const previousSelectedId = get().selectedTraceId

    set({ traceLoading: true, traceError: null })

    try {
      const [traceSummary, traces] = await Promise.all([
        fetchTraceSummary(),
        fetchTraces(),
      ])

      const nextSelectedId = traces.some(
        (trace) => trace.id === previousSelectedId
      )
        ? previousSelectedId
        : null

      set({
        traces,
        traceSummary,
        selectedTraceId: nextSelectedId,
        selectedTrace:
          nextSelectedId != null && get().selectedTrace?.id === nextSelectedId
            ? get().selectedTrace
            : null,
      })

      if (nextSelectedId != null) {
        await get().selectTrace(nextSelectedId)
      } else {
        set({ selectedTrace: null, traceLoading: false })
      }
    } catch (err: unknown) {
      set({
        traceLoading: false,
        traceError:
          err instanceof Error ? err.message : "Failed to load traces",
      })
    }
  },

  selectTrace: async (traceId: string) => {
    set({ selectedTraceId: traceId, traceLoading: true, traceError: null })

    try {
      const selectedTrace = await fetchTrace(traceId)
      if (get().selectedTraceId !== traceId) {
        return
      }

      set({ selectedTrace, traceLoading: false })
    } catch (err: unknown) {
      if (get().selectedTraceId !== traceId) {
        return
      }

      set({
        selectedTrace: null,
        traceLoading: false,
        traceError:
          err instanceof Error ? err.message : "Failed to load trace detail",
      })
    }
  },

  clearSelection: () => {
    set({ selectedTraceId: null, selectedTrace: null, traceError: null })
  },
}))
