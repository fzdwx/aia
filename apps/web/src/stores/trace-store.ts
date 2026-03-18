import { create } from "zustand"
import { fetchTrace, fetchTraceOverview } from "@/lib/api"
import type { TraceListItem, TraceRecord, TraceSummary } from "@/lib/types"

const TRACE_PAGE_SIZE = 12

export type TraceView = "conversation" | "compression"

function requestKindForView(view: TraceView) {
  return view === "compression" ? "compression" : "completion"
}

let inflightOverviewKey: string | null = null
let inflightOverviewPromise: Promise<{
  traceSummary: TraceSummary
  traces: TraceListItem[]
  tracePage: number
  tracePageSize: number
  totalTraceItems: number
}> | null = null

type TraceStore = {
  traces: TraceListItem[]
  traceView: TraceView
  tracePage: number
  tracePageSize: number
  totalTraceItems: number
  selectedTraceId: string | null
  selectedTrace: TraceRecord | null
  traceSummary: TraceSummary | null
  traceLoading: boolean
  traceError: string | null
  refreshTraces: (options?: {
    page?: number
    view?: TraceView
  }) => Promise<void>
  switchTraceView: (view: TraceView) => Promise<void>
  selectTrace: (traceId: string) => Promise<void>
  clearSelection: () => void
}

export const useTraceStore = create<TraceStore>((set, get) => ({
  traces: [],
  traceView: "conversation",
  tracePage: 1,
  tracePageSize: TRACE_PAGE_SIZE,
  totalTraceItems: 0,
  selectedTraceId: null,
  selectedTrace: null,
  traceSummary: null,
  traceLoading: false,
  traceError: null,

  refreshTraces: async (options) => {
    const previousSelectedId = get().selectedTraceId
    const tracePage = options?.page ?? get().tracePage
    const traceView = options?.view ?? get().traceView
    const tracePageSize = get().tracePageSize
    const requestKind = requestKindForView(traceView)

    set({ traceLoading: true, traceError: null })

    try {
      const overviewKey = `${traceView}:${tracePage}:${tracePageSize}`
      const overviewPromise =
        inflightOverviewKey === overviewKey && inflightOverviewPromise
          ? inflightOverviewPromise
          : fetchTraceOverview({
              page: tracePage,
              page_size: tracePageSize,
              request_kind: requestKind,
            }).then((overview) => ({
              traceSummary: overview.summary,
              traces: overview.page.items,
              tracePage: overview.page.page,
              tracePageSize: overview.page.page_size,
              totalTraceItems: overview.page.total_items,
            }))

      inflightOverviewKey = overviewKey
      inflightOverviewPromise = overviewPromise

      const {
        traceSummary,
        traces,
        tracePage: nextTracePage,
        tracePageSize: nextTracePageSize,
        totalTraceItems,
      } = await overviewPromise

      if (inflightOverviewKey === overviewKey) {
        inflightOverviewKey = null
        inflightOverviewPromise = null
      }

      const nextSelectedId = traces.some(
        (trace) => trace.id === previousSelectedId
      )
        ? previousSelectedId
        : null

      set({
        traces,
        traceView,
        tracePage: nextTracePage,
        tracePageSize: nextTracePageSize,
        totalTraceItems,
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
      inflightOverviewKey = null
      inflightOverviewPromise = null
      set({
        traceLoading: false,
        traceError:
          err instanceof Error ? err.message : "Failed to load traces",
      })
    }
  },

  switchTraceView: async (view) => {
    set({
      traceView: view,
      tracePage: 1,
      selectedTraceId: null,
      selectedTrace: null,
      traceError: null,
    })
    await get().refreshTraces({ page: 1, view })
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
