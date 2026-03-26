import { create } from "zustand"
import {
  fetchCurrentTurn,
  fetchHistory,
  fetchSessionInfo,
  fetchSessions as apiFetchSessions,
  createSession as apiCreateSession,
  deleteSession as apiDeleteSession,
  submitTurn as apiSubmitTurn,
  cancelTurn as apiCancelTurn,
} from "@/lib/api"
import {
  createIdleScheduler,
  type IdleCanceller,
  type IdleScheduler,
} from "@/lib/idle"
import { setActiveWorkspaceRoot } from "@/lib/tool-display"
import type {
  ChatState,
  ContextCompressionNotice,
  SessionListItem,
  SseEvent,
  StreamingTurn,
  TurnLifecycle,
  TurnStatus,
} from "@/lib/types"
import {
  applyStreamEventToBlocks,
  createPendingStreamingTurn,
  currentTurnToStreamingTurn,
  withStreamingStatus,
} from "@/stores/chat-sse-projection"
import { useProviderRegistryStore } from "@/stores/provider-registry-store"
import {
  clearSessionSettingsState,
  hydrateSessionSettingsForSession,
} from "@/stores/session-settings-coordinator"
import { usePendingQuestionStore } from "@/stores/pending-question-store"

const SESSION_HISTORY_PAGE_SIZE = 5
const INITIAL_SESSION_HISTORY_PAGE_SIZE = 1
const DEFERRED_SECOND_TURN_PAGE_SIZE = SESSION_HISTORY_PAGE_SIZE
const MAX_CACHED_SESSION_SNAPSHOTS = 24
export const NEW_PROVIDER_SETTINGS_KEY = "__new_provider__"

const defaultIdleScheduler = createIdleScheduler()

type IdleHandle = number

type SessionSnapshot = {
  latestTurn: TurnLifecycle | null
  streamingTurn: StreamingTurn | null
  chatState: ChatState
  contextPressure: number | null
  lastCompression: ContextCompressionNotice | null
}

const EMPTY_SESSION_SNAPSHOT: SessionSnapshot = {
  latestTurn: null,
  streamingTurn: null,
  chatState: "idle",
  contextPressure: null,
  lastCompression: null,
}

function latestTurn(turns: TurnLifecycle[]): TurnLifecycle | null {
  return turns.length > 0 ? (turns[turns.length - 1] ?? null) : null
}

function snapshotFromState(
  state: Pick<
    ChatStore,
    | "turns"
    | "streamingTurn"
    | "chatState"
    | "contextPressure"
    | "lastCompression"
  >
): SessionSnapshot {
  return {
    latestTurn: latestTurn(state.turns),
    streamingTurn: state.streamingTurn,
    chatState: state.chatState,
    contextPressure: state.contextPressure,
    lastCompression: state.lastCompression,
  }
}

function mergeTurnsById(
  olderTurns: TurnLifecycle[],
  newerTurns: TurnLifecycle[]
): TurnLifecycle[] {
  const seen = new Set<string>()
  const merged: TurnLifecycle[] = []

  for (const turn of [...olderTurns, ...newerTurns]) {
    if (seen.has(turn.turn_id)) continue
    seen.add(turn.turn_id)
    merged.push(turn)
  }

  return merged
}

function applySessionSnapshot(
  snapshot: SessionSnapshot,
  sessionHydrating: boolean
): Pick<
  ChatStore,
  | "sessionHydrating"
  | "turns"
  | "historyHasMore"
  | "historyNextBeforeTurnId"
  | "streamingTurn"
  | "chatState"
  | "contextPressure"
  | "lastCompression"
  | "error"
> {
  return {
    sessionHydrating,
    turns: snapshot.latestTurn ? [snapshot.latestTurn] : [],
    historyHasMore: false,
    historyNextBeforeTurnId: null,
    streamingTurn: snapshot.streamingTurn,
    chatState: snapshot.chatState,
    contextPressure: snapshot.contextPressure,
    lastCompression: snapshot.lastCompression,
    error: null,
  }
}

function upsertSessionSnapshot(
  snapshots: Record<string, SessionSnapshot>,
  sessionId: string | null,
  snapshot: SessionSnapshot,
  sessionOrder?: SessionListItem[]
): Record<string, SessionSnapshot> {
  if (!sessionId) return snapshots

  const nextSnapshots = {
    ...snapshots,
    [sessionId]: snapshot,
  }

  if (!sessionOrder || sessionOrder.length <= MAX_CACHED_SESSION_SNAPSHOTS) {
    return nextSnapshots
  }

  const allowedIds = new Set(
    sessionOrder
      .slice(Math.max(0, sessionOrder.length - MAX_CACHED_SESSION_SNAPSHOTS))
      .map((session) => session.id)
  )
  allowedIds.add(sessionId)

  return Object.fromEntries(
    Object.entries(nextSnapshots).filter(([id]) => allowedIds.has(id))
  )
}

function trimSessionSnapshotsToKnownSessions(
  snapshots: Record<string, SessionSnapshot>,
  sessions: SessionListItem[]
): Record<string, SessionSnapshot> {
  const allowedIds = new Set(sessions.map((session) => session.id))
  return Object.fromEntries(
    Object.entries(snapshots).filter(([id]) => allowedIds.has(id))
  )
}

type ChatStore = {
  sessions: SessionListItem[]
  activeSessionId: string | null
  sessionHydrating: boolean
  turns: TurnLifecycle[]
  historyHasMore: boolean
  historyNextBeforeTurnId: string | null
  historyLoadingMore: boolean
  streamingTurn: StreamingTurn | null
  chatState: ChatState
  error: string | null
  contextPressure: number | null
  lastCompression: ContextCompressionNotice | null
  _pendingPrompt: string | null
  _sessionSnapshots: Record<string, SessionSnapshot>
  initialize: () => void
  handleSseEvent: (event: SseEvent) => void
  submitTurn: (prompt: string) => void
  cancelTurn: () => Promise<void>
  fetchSessions: () => Promise<void>
  createSession: () => Promise<void>
  switchSession: (id: string) => Promise<void>
  loadOlderTurns: () => Promise<void>
  deleteSession: (id: string) => Promise<void>
}

let latestSessionLoadId = 0
let pendingHistoryHydrationAbort: AbortController | null = null
let pendingHistoryHydrationIdleHandle: IdleHandle | null = null
let scheduleIdleWork: IdleScheduler = defaultIdleScheduler.schedule
let cancelIdleWork: IdleCanceller = defaultIdleScheduler.cancel

export function __setIdleSchedulerForTests(
  scheduler: {
    schedule: IdleScheduler
    cancel: IdleCanceller
  } | null
) {
  if (scheduler) {
    scheduleIdleWork = scheduler.schedule
    cancelIdleWork = scheduler.cancel
    return
  }
  scheduleIdleWork = defaultIdleScheduler.schedule
  cancelIdleWork = defaultIdleScheduler.cancel
}

function cancelPendingHistoryHydration() {
  pendingHistoryHydrationAbort?.abort()
  pendingHistoryHydrationAbort = null
  if (pendingHistoryHydrationIdleHandle != null) {
    cancelIdleWork(pendingHistoryHydrationIdleHandle)
    pendingHistoryHydrationIdleHandle = null
  }
}

function scheduleIdle(callback: () => void): IdleHandle {
  return scheduleIdleWork(callback)
}

export const useChatStore = create<ChatStore>((set, get) => {
  async function hydrateSession(id: string) {
    cancelPendingHistoryHydration()
    const loadId = ++latestSessionLoadId
    const cachedSnapshot = get()._sessionSnapshots[id] ?? EMPTY_SESSION_SNAPSHOT
    const historyPagePromise = fetchHistory({
      sessionId: id,
      limit: INITIAL_SESSION_HISTORY_PAGE_SIZE,
    })
    const currentTurnPromise = fetchCurrentTurn(id)
    const sessionInfoPromise = fetchSessionInfo(id)
    setActiveWorkspaceRoot(null)

    set((state) => ({
      activeSessionId: id,
      ...applySessionSnapshot(cachedSnapshot, true),
      _sessionSnapshots: upsertSessionSnapshot(
        state._sessionSnapshots,
        state.activeSessionId,
        snapshotFromState(state),
        state.sessions
      ),
    }))

    sessionInfoPromise
      .then((info) => {
        if (loadId !== latestSessionLoadId) return
        setActiveWorkspaceRoot(info.workspace_root)
        set((state) => {
          const snapshot = {
            ...(state._sessionSnapshots[id] ?? cachedSnapshot),
            contextPressure: info.pressure_ratio,
          }
          return {
            contextPressure: info.pressure_ratio,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              id,
              snapshot,
              state.sessions
            ),
          }
        })
      })
      .catch(() => {})

    try {
      const [historyPage, currentTurn] = await Promise.all([
        historyPagePromise,
        currentTurnPromise,
      ])

      if (loadId !== latestSessionLoadId) {
        return
      }

      const hydratedSnapshot: SessionSnapshot = {
        latestTurn: latestTurn(historyPage.turns),
        streamingTurn: currentTurn
          ? currentTurnToStreamingTurn(currentTurn)
          : null,
        chatState: currentTurn ? "active" : "idle",
        contextPressure: get().contextPressure,
        lastCompression: null,
      }

      set((state) => ({
        ...applySessionSnapshot(hydratedSnapshot, false),
        historyHasMore: historyPage.has_more,
        historyNextBeforeTurnId: historyPage.next_before_turn_id,
        _sessionSnapshots: upsertSessionSnapshot(
          state._sessionSnapshots,
          id,
          hydratedSnapshot,
          state.sessions
        ),
      }))

      if (historyPage.has_more && historyPage.next_before_turn_id) {
        const beforeTurnId = historyPage.next_before_turn_id
        const abortController = new AbortController()
        pendingHistoryHydrationAbort = abortController
        pendingHistoryHydrationIdleHandle = scheduleIdle(() => {
          pendingHistoryHydrationIdleHandle = null
          void fetchHistory({
            sessionId: id,
            beforeTurnId,
            limit: DEFERRED_SECOND_TURN_PAGE_SIZE,
            signal: abortController.signal,
          })
            .then((olderHistoryPage) => {
              if (
                abortController.signal.aborted ||
                loadId !== latestSessionLoadId ||
                get().activeSessionId !== id
              ) {
                return
              }

              const existingTurns = get().turns
              const turns = mergeTurnsById(
                olderHistoryPage.turns,
                existingTurns
              )
              const nextSnapshot: SessionSnapshot = {
                latestTurn: latestTurn(turns),
                streamingTurn: get().streamingTurn,
                chatState: get().chatState,
                contextPressure: get().contextPressure,
                lastCompression: get().lastCompression,
              }
              set((state) => ({
                turns,
                historyHasMore: olderHistoryPage.has_more,
                historyNextBeforeTurnId: olderHistoryPage.next_before_turn_id,
                _sessionSnapshots: upsertSessionSnapshot(
                  state._sessionSnapshots,
                  id,
                  nextSnapshot,
                  state.sessions
                ),
              }))
            })
            .catch((error: unknown) => {
              if (
                error instanceof DOMException &&
                error.name === "AbortError"
              ) {
                return
              }
            })
            .finally(() => {
              if (pendingHistoryHydrationAbort === abortController) {
                pendingHistoryHydrationAbort = null
              }
            })
        })
      }
    } catch {
      if (loadId !== latestSessionLoadId) {
        return
      }
      set({ sessionHydrating: false })
    }
  }

  async function recoverStreamingTurn(sessionId: string) {
    try {
      const currentTurn = await fetchCurrentTurn(sessionId)
      if (!currentTurn) return
      if (get().activeSessionId !== sessionId) return
      set((state) => {
        const nextStreamingTurn = currentTurnToStreamingTurn(currentTurn)
        return {
          chatState: "active",
          streamingTurn: nextStreamingTurn,
          _sessionSnapshots: upsertSessionSnapshot(
            state._sessionSnapshots,
            sessionId,
            {
              ...(state._sessionSnapshots[sessionId] ?? EMPTY_SESSION_SNAPSHOT),
              streamingTurn: nextStreamingTurn,
              chatState: "active",
            },
            state.sessions
          ),
        }
      })
    } catch (error) {
      void error
    }
  }

  function refreshActiveSessionPressure(sessionId: string | null) {
    fetchSessionInfo(sessionId ?? undefined)
      .then((info) =>
        set((state) => {
          const snapshot = state._sessionSnapshots[sessionId ?? ""]
          if (!sessionId || !snapshot) {
            return { contextPressure: info.pressure_ratio }
          }
          return {
            contextPressure: info.pressure_ratio,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              sessionId,
              {
                ...snapshot,
                contextPressure: info.pressure_ratio,
              },
              state.sessions
            ),
          }
        })
      )
      .catch(() => {})
  }

  return {
    sessions: [],
    activeSessionId: null,
    sessionHydrating: false,
    turns: [],
    historyHasMore: false,
    historyNextBeforeTurnId: null,
    historyLoadingMore: false,
    streamingTurn: null,
    chatState: "idle",
    error: null,
    contextPressure: null,
    lastCompression: null,
    _pendingPrompt: null,
    _sessionSnapshots: {},

    initialize: () => {
      apiFetchSessions()
        .then((sessions) => {
          const activeId = sessions[0]?.id ?? null
          set({ sessions, activeSessionId: activeId })
          if (activeId) {
            void hydrateSession(activeId)
            void hydrateSessionSettingsForSession(activeId)
            void usePendingQuestionStore.getState().hydrateForSession(activeId)
          }
        })
        .catch(() => {})

      void useProviderRegistryStore.getState().refreshProviders()
    },

    handleSseEvent: (event: SseEvent) => {
      const activeId = get().activeSessionId

      function setStreamingTurnForActiveSession(
        updater: (streamingTurn: StreamingTurn) => StreamingTurn | null
      ) {
        set((state) => {
          if (!activeId || !state.streamingTurn) return state
          const nextStreamingTurn = updater(state.streamingTurn)
          if (!nextStreamingTurn) return state
          return {
            streamingTurn: nextStreamingTurn,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              activeId,
              {
                ...(state._sessionSnapshots[activeId] ??
                  EMPTY_SESSION_SNAPSHOT),
                streamingTurn: nextStreamingTurn,
              },
              state.sessions
            ),
          }
        })
      }

      switch (event.type) {
        case "current_turn_started": {
          if (event.data.session_id !== activeId) break
          const nextStreamingTurn = currentTurnToStreamingTurn(event.data)
          set((state) => ({
            _pendingPrompt: null,
            chatState: "active",
            streamingTurn: nextStreamingTurn,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              activeId,
              {
                ...(state._sessionSnapshots[activeId ?? ""] ??
                  EMPTY_SESSION_SNAPSHOT),
                streamingTurn: nextStreamingTurn,
                chatState: "active",
              },
              state.sessions
            ),
          }))
          break
        }
        case "status": {
          if (event.data.session_id !== activeId) break

          const status = event.data.status as TurnStatus
          if (status === "waiting") {
            const prev = get().streamingTurn
            if (prev) {
              set((state) => {
                const nextStreamingTurn = withStreamingStatus(prev, "waiting")
                return {
                  _pendingPrompt: null,
                  chatState: "active",
                  streamingTurn: nextStreamingTurn,
                  _sessionSnapshots: upsertSessionSnapshot(
                    state._sessionSnapshots,
                    activeId,
                    {
                      ...(state._sessionSnapshots[activeId ?? ""] ??
                        EMPTY_SESSION_SNAPSHOT),
                      streamingTurn: nextStreamingTurn,
                      chatState: "active",
                    },
                    state.sessions
                  ),
                }
              })
            } else {
              const prompt = get()._pendingPrompt
              if (prompt) {
                set((state) => {
                  const nextStreamingTurn = createPendingStreamingTurn(prompt)
                  return {
                    _pendingPrompt: null,
                    chatState: "active",
                    streamingTurn: nextStreamingTurn,
                    _sessionSnapshots: upsertSessionSnapshot(
                      state._sessionSnapshots,
                      activeId,
                      {
                        ...(state._sessionSnapshots[activeId ?? ""] ??
                          EMPTY_SESSION_SNAPSHOT),
                        streamingTurn: nextStreamingTurn,
                        chatState: "active",
                      },
                      state.sessions
                    ),
                  }
                })
              } else if (activeId) {
                set({ _pendingPrompt: null, chatState: "active" })
                void recoverStreamingTurn(activeId)
              }
            }
          } else {
            const prev = get().streamingTurn
            if (prev) {
              set((state) => {
                const nextStreamingTurn = withStreamingStatus(prev, status)
                return {
                  streamingTurn: nextStreamingTurn,
                  _sessionSnapshots: upsertSessionSnapshot(
                    state._sessionSnapshots,
                    activeId,
                    {
                      ...(state._sessionSnapshots[activeId ?? ""] ??
                        EMPTY_SESSION_SNAPSHOT),
                      streamingTurn: nextStreamingTurn,
                    },
                    state.sessions
                  ),
                }
              })
            } else if (activeId) {
              void recoverStreamingTurn(activeId)
            }
          }
          break
        }
        case "stream": {
          if (event.data.session_id !== activeId) break

          const data = event.data
          const prev = get().streamingTurn
          if (!prev) {
            if (activeId) {
              void recoverStreamingTurn(activeId)
            }
            break
          }

          const blocks = applyStreamEventToBlocks(prev.blocks, data)
          setStreamingTurnForActiveSession((current) => ({
            ...current,
            blocks,
          }))
          break
        }
        case "turn_completed": {
          if (event.data.session_id !== activeId) break

          set((state) => {
            const turns = [...state.turns, event.data]
            const isWaitingForQuestion =
              event.data.outcome === "waiting_for_question"
            const snapshot: SessionSnapshot = {
              latestTurn: event.data,
              streamingTurn: null,
              chatState: isWaitingForQuestion ? "active" : "idle",
              contextPressure: state.contextPressure,
              lastCompression: null,
            }
            return {
              turns,
              streamingTurn: null,
              chatState: isWaitingForQuestion ? "active" : "idle",
              error: null,
              lastCompression: null,
              _sessionSnapshots: upsertSessionSnapshot(
                state._sessionSnapshots,
                activeId,
                snapshot,
                state.sessions
              ),
            }
          })
          void usePendingQuestionStore.getState().hydrateForSession(activeId)
          refreshActiveSessionPressure(activeId)
          break
        }
        case "context_compressed": {
          if (event.data.session_id !== activeId) break
          set((state) => {
            const snapshot = state._sessionSnapshots[activeId ?? ""]
            return {
              lastCompression: event.data,
              _sessionSnapshots:
                activeId && snapshot
                  ? upsertSessionSnapshot(
                      state._sessionSnapshots,
                      activeId,
                      {
                        ...snapshot,
                        lastCompression: event.data,
                      },
                      state.sessions
                    )
                  : state._sessionSnapshots,
            }
          })
          refreshActiveSessionPressure(activeId)
          break
        }
        case "sync_required": {
          if (!activeId) break
          void get()
            .fetchSessions()
            .catch(() => {})
          void hydrateSession(activeId)
          void usePendingQuestionStore.getState().hydrateForSession(activeId)
          break
        }
        case "error": {
          if (event.data.session_id !== activeId) break
          const latestTurn = get().turns[get().turns.length - 1]
          if (
            !get().streamingTurn &&
            latestTurn?.failure_message === event.data.message
          ) {
            break
          }
          const streamingTurn = get().streamingTurn
          const isCancelledError = event.data.message.includes("已取消")
          if (streamingTurn && isCancelledError) {
            set((state) => {
              const nextStreamingTurn = {
                ...streamingTurn,
                status: "cancelled" as const,
              }
              const snapshot = state._sessionSnapshots[activeId ?? ""]
              return {
                streamingTurn: nextStreamingTurn,
                chatState: "idle",
                error: null,
                _sessionSnapshots:
                  activeId && snapshot
                    ? upsertSessionSnapshot(
                        state._sessionSnapshots,
                        activeId,
                        {
                          ...snapshot,
                          streamingTurn: nextStreamingTurn,
                          chatState: "idle",
                        },
                        state.sessions
                      )
                    : state._sessionSnapshots,
              }
            })
            break
          }
          set((state) => {
            const snapshot = state._sessionSnapshots[activeId ?? ""]
            return {
              error: event.data.message,
              streamingTurn: null,
              chatState: "idle",
              _sessionSnapshots:
                activeId && snapshot
                  ? upsertSessionSnapshot(
                      state._sessionSnapshots,
                      activeId,
                      {
                        ...snapshot,
                        streamingTurn: null,
                        chatState: "idle",
                      },
                      state.sessions
                    )
                  : state._sessionSnapshots,
            }
          })
          break
        }
        case "turn_cancelled": {
          if (event.data.session_id !== activeId) break
          const prev = get().streamingTurn
          if (!prev) break
          set((state) => {
            const nextStreamingTurn = { ...prev, status: "cancelled" as const }
            const snapshot = state._sessionSnapshots[activeId ?? ""]
            return {
              streamingTurn: nextStreamingTurn,
              chatState: "idle",
              error: null,
              _sessionSnapshots:
                activeId && snapshot
                  ? upsertSessionSnapshot(
                      state._sessionSnapshots,
                      activeId,
                      {
                        ...snapshot,
                        streamingTurn: nextStreamingTurn,
                        chatState: "idle",
                      },
                      state.sessions
                    )
                  : state._sessionSnapshots,
            }
          })
          break
        }
        case "session_created": {
          get().fetchSessions()
          break
        }
        case "session_deleted": {
          const deletedId = event.data.session_id
          const wasActive = get().activeSessionId === deletedId
          let nextActiveId: string | null = null

          set((state) => {
            const sessions = state.sessions.filter((s) => s.id !== deletedId)
            const nextSnapshots = trimSessionSnapshotsToKnownSessions(
              state._sessionSnapshots,
              sessions
            )

            if (!wasActive) {
              return { sessions, _sessionSnapshots: nextSnapshots }
            }

            nextActiveId = sessions[0]?.id ?? null
            const nextSnapshot =
              (nextActiveId ? nextSnapshots[nextActiveId] : null) ??
              EMPTY_SESSION_SNAPSHOT

            return {
              sessions,
              activeSessionId: nextActiveId,
              ...applySessionSnapshot(nextSnapshot, nextActiveId != null),
              historyLoadingMore: false,
              _pendingPrompt: null,
              _sessionSnapshots: nextSnapshots,
            }
          })

          cancelPendingHistoryHydration()
          if (wasActive && nextActiveId) {
            void hydrateSession(nextActiveId)
            void hydrateSessionSettingsForSession(nextActiveId)
            void usePendingQuestionStore
              .getState()
              .hydrateForSession(nextActiveId)
          } else if (wasActive) {
            clearSessionSettingsState()
            usePendingQuestionStore.getState().clear()
          }
          break
        }
      }
    },

    submitTurn: (prompt: string) => {
      if (get().chatState === "active") return
      const sessionId = get().activeSessionId
      if (!sessionId) return

      set((state) => {
        const nextStreamingTurn: StreamingTurn = {
          userMessage: prompt,
          status: "waiting",
          blocks: [],
        }
        const snapshot =
          state._sessionSnapshots[sessionId] ?? EMPTY_SESSION_SNAPSHOT
        const nextState = {
          ...state,
          error: null,
          _pendingPrompt: prompt,
          chatState: "active" as const,
          lastCompression: null,
          streamingTurn: nextStreamingTurn,
        }
        return {
          error: null,
          _pendingPrompt: prompt,
          chatState: "active",
          lastCompression: null,
          streamingTurn: nextStreamingTurn,
          _sessionSnapshots: upsertSessionSnapshot(
            state._sessionSnapshots,
            sessionId,
            {
              ...snapshotFromState(nextState),
              latestTurn: snapshot.latestTurn,
            },
            state.sessions
          ),
        }
      })
      apiSubmitTurn(prompt, sessionId).catch((err: unknown) => {
        void usePendingQuestionStore.getState().hydrateForSession(sessionId)
        set((state) => {
          const nextState = {
            ...state,
            error: err instanceof Error ? err.message : "Network error",
            _pendingPrompt: null,
            streamingTurn: null,
            chatState: "idle" as const,
          }
          return {
            error: err instanceof Error ? err.message : "Network error",
            _pendingPrompt: null,
            streamingTurn: null,
            chatState: "idle",
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              sessionId,
              snapshotFromState(nextState),
              state.sessions
            ),
          }
        })
      })
    },

    cancelTurn: async () => {
      const sessionId = get().activeSessionId
      const streamingTurn = get().streamingTurn
      if (!sessionId || !streamingTurn) return

      try {
        const cancelled = await apiCancelTurn(sessionId)
        if (!cancelled) return
        set((state) => {
          const nextStreamingTurn = {
            ...streamingTurn,
            status: "cancelled" as const,
          }
          const nextState = {
            ...state,
            streamingTurn: nextStreamingTurn,
            chatState: "idle" as const,
            error: null,
          }
          return {
            streamingTurn: nextStreamingTurn,
            chatState: "idle",
            error: null,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              sessionId,
              snapshotFromState(nextState),
              state.sessions
            ),
          }
        })
      } catch (err) {
        set({
          error: err instanceof Error ? err.message : "Cancel failed",
        })
      }
    },

    fetchSessions: async () => {
      const sessions = await apiFetchSessions()
      set((state) => ({
        sessions,
        _sessionSnapshots: trimSessionSnapshotsToKnownSessions(
          state._sessionSnapshots,
          sessions
        ),
      }))
    },

    createSession: async () => {
      const session = await apiCreateSession()
      set((state) => ({
        sessions: [...state.sessions, session],
        _sessionSnapshots: trimSessionSnapshotsToKnownSessions(
          state._sessionSnapshots,
          [...state.sessions, session]
        ),
      }))
      await get().switchSession(session.id)
    },

    switchSession: async (id: string) => {
      if (id === get().activeSessionId && !get().sessionHydrating) {
        return
      }
      cancelPendingHistoryHydration()
      await hydrateSession(id)
      await hydrateSessionSettingsForSession(id)
      await usePendingQuestionStore.getState().hydrateForSession(id)
    },

    loadOlderTurns: async () => {
      cancelPendingHistoryHydration()
      const state = get()
      const sessionId = state.activeSessionId
      const beforeTurnId = state.historyNextBeforeTurnId
      if (!sessionId || get().historyLoadingMore) return

      if (!beforeTurnId) return

      set({ historyLoadingMore: true })
      try {
        const historyPage = await fetchHistory({
          sessionId,
          beforeTurnId,
          limit: SESSION_HISTORY_PAGE_SIZE,
        })

        if (get().activeSessionId !== sessionId) {
          set({ historyLoadingMore: false })
          return
        }

        set((state) => {
          if (state.activeSessionId !== sessionId) {
            return { historyLoadingMore: false }
          }

          const turns = mergeTurnsById(historyPage.turns, state.turns)
          return {
            turns,
            historyHasMore: historyPage.has_more,
            historyNextBeforeTurnId: historyPage.next_before_turn_id,
            historyLoadingMore: false,
            _sessionSnapshots: upsertSessionSnapshot(
              state._sessionSnapshots,
              sessionId,
              {
                ...(state._sessionSnapshots[sessionId] ??
                  EMPTY_SESSION_SNAPSHOT),
                latestTurn: latestTurn(turns),
              },
              state.sessions
            ),
          }
        })
      } catch {
        set({ historyLoadingMore: false })
      }
    },

    deleteSession: async (id: string) => {
      cancelPendingHistoryHydration()
      const deletedWasActive = get().activeSessionId === id
      await apiDeleteSession(id)
      const state = get()
      const remaining = state.sessions.filter((s) => s.id !== id)
      const nextSnapshots = { ...state._sessionSnapshots }
      delete nextSnapshots[id]
      set({ sessions: remaining, _sessionSnapshots: nextSnapshots })

      if (deletedWasActive) {
        const next = remaining[0]
        if (next) {
          await get().switchSession(next.id)
        } else {
          clearSessionSettingsState()
          usePendingQuestionStore.getState().clear()
          set({
            activeSessionId: null,
            sessionHydrating: false,
            turns: [],
            historyHasMore: false,
            historyNextBeforeTurnId: null,
            historyLoadingMore: false,
            streamingTurn: null,
            chatState: "idle",
            lastCompression: null,
            contextPressure: null,
          })
        }
      }
    },
  }
})
