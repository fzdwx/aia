import { create } from "zustand"

import {
  cancelPendingQuestion as apiCancelPendingQuestion,
  fetchPendingQuestion,
  resolvePendingQuestion as apiResolvePendingQuestion,
} from "@/lib/api"
import type { PendingQuestionResponse, QuestionRequest, QuestionResult } from "@/lib/types"

type PendingQuestionStore = {
  pendingQuestion: QuestionRequest | null
  hydrating: boolean
  submitting: boolean
  error: string | null
  hydrateForSession: (sessionId: string) => Promise<void>
  clear: () => void
  submitResult: (sessionId: string, result: QuestionResult) => Promise<void>
  cancel: (sessionId: string) => Promise<void>
}

let latestHydrationRequestId = 0
let latestMutationRequestId = 0

function toPendingQuestion(response: PendingQuestionResponse): QuestionRequest | null {
  return response.pending ? (response.request ?? null) : null
}

export const usePendingQuestionStore = create<PendingQuestionStore>((set) => ({
  pendingQuestion: null,
  hydrating: false,
  submitting: false,
  error: null,

  hydrateForSession: async (sessionId: string) => {
    const requestId = ++latestHydrationRequestId
    set({ hydrating: true, error: null })
    try {
      const response = await fetchPendingQuestion(sessionId)
      if (requestId !== latestHydrationRequestId) return
      set({
        hydrating: false,
        pendingQuestion: toPendingQuestion(response),
      })
    } catch (error) {
      if (requestId !== latestHydrationRequestId) return
      set({
        hydrating: false,
        error:
          error instanceof Error
            ? error.message
            : "Failed to load pending question",
      })
    }
  },

  clear: () => {
    latestHydrationRequestId += 1
    latestMutationRequestId += 1
    set({
      pendingQuestion: null,
      hydrating: false,
      submitting: false,
      error: null,
    })
  },

  submitResult: async (sessionId: string, result: QuestionResult) => {
    const requestId = ++latestMutationRequestId
    set({ submitting: true, error: null })
    try {
      await apiResolvePendingQuestion({ session_id: sessionId, result })
      if (requestId !== latestMutationRequestId) return
      set({ pendingQuestion: null, submitting: false })
    } catch (error) {
      if (requestId !== latestMutationRequestId) throw error
      set({
        submitting: false,
        error:
          error instanceof Error
            ? error.message
            : "Failed to resolve pending question",
      })
      throw error
    }
  },

  cancel: async (sessionId: string) => {
    const requestId = ++latestMutationRequestId
    set({ submitting: true, error: null })
    try {
      await apiCancelPendingQuestion(sessionId)
      if (requestId !== latestMutationRequestId) return
      set({ pendingQuestion: null, submitting: false })
    } catch (error) {
      if (requestId !== latestMutationRequestId) throw error
      set({
        submitting: false,
        error:
          error instanceof Error ? error.message : "Failed to cancel pending question",
      })
      throw error
    }
  },
}))
