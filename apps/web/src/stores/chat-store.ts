import { create } from "zustand"
import {
  fetchCurrentTurn,
  fetchHistory,
  fetchSessionInfo,
  fetchSessions as apiFetchSessions,
  fetchQueue as apiFetchQueue,
  createSession as apiCreateSession,
  deleteSession as apiDeleteSession,
  submitTurn as apiSubmitTurn,
  sendMessage as apiSendMessage,
  cancelTurn as apiCancelTurn,
  interruptTurn as apiInterruptTurn,
  deleteQueuedMessage as apiDeleteQueuedMessage,
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
  QueuedMessage,
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
const SESSION_TITLE_ANIMATION_TICK_MS = 36

const defaultIdleScheduler = createIdleScheduler()

type IdleHandle = number

type SessionSnapshot = {
  latestTurn: TurnLifecycle | null
  streamingTurn: StreamingTurn | null
  chatState: ChatState
  contextPressure: number | null
  lastCompression: ContextCompressionNotice | null
  messageQueue: QueuedMessage[]
}

type SessionTitleAnimation = {
  targetTitle: string
  renderedTitle: string
  animating: boolean
}

const EMPTY_SESSION_SNAPSHOT: SessionSnapshot = {
  latestTurn: null,
  streamingTurn: null,
  chatState: "idle",
  contextPressure: null,
  lastCompression: null,
  messageQueue: [],
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
    | "messageQueue"
  >
): SessionSnapshot {
  return {
    latestTurn: latestTurn(state.turns),
    streamingTurn: state.streamingTurn,
    chatState: state.chatState,
    contextPressure: state.contextPressure,
    lastCompression: state.lastCompression,
    messageQueue: state.messageQueue,
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
  | "messageQueue"
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
    messageQueue: snapshot.messageQueue,
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

let sessionSnapshotsCache: Record<string, SessionSnapshot> = {}

function getSessionSnapshot(sessionId: string | null): SessionSnapshot | null {
  if (!sessionId) return null
  return sessionSnapshotsCache[sessionId] ?? null
}

function cacheSessionSnapshot(
  sessionId: string | null,
  snapshot: SessionSnapshot,
  sessionOrder?: SessionListItem[]
) {
  sessionSnapshotsCache = upsertSessionSnapshot(
    sessionSnapshotsCache,
    sessionId,
    snapshot,
    sessionOrder
  )
}

function updateSessionSnapshot(
  sessionId: string | null,
  updater: (snapshot: SessionSnapshot) => SessionSnapshot,
  sessionOrder?: SessionListItem[]
) {
  if (!sessionId) return

  cacheSessionSnapshot(
    sessionId,
    updater(getSessionSnapshot(sessionId) ?? EMPTY_SESSION_SNAPSHOT),
    sessionOrder
  )
}

function trimSessionSnapshotCache(sessions: SessionListItem[]) {
  sessionSnapshotsCache = trimSessionSnapshotsToKnownSessions(
    sessionSnapshotsCache,
    sessions
  )
}

function removeSessionSnapshot(sessionId: string) {
  const { [sessionId]: _removed, ...rest } = sessionSnapshotsCache
  sessionSnapshotsCache = rest
}

export function __resetSessionSnapshotCacheForTests() {
  sessionSnapshotsCache = {}
}

export function __getSessionSnapshotForTests(sessionId: string) {
  return getSessionSnapshot(sessionId)
}

export function __setSessionSnapshotForTests(
  sessionId: string,
  snapshot: SessionSnapshot
) {
  sessionSnapshotsCache = {
    ...sessionSnapshotsCache,
    [sessionId]: snapshot,
  }
}

type ChatStore = {
  sessions: SessionListItem[]
  sessionTitleAnimations: Record<string, SessionTitleAnimation>
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
  messageQueue: QueuedMessage[]
  _pendingPrompt: string | null
  initialize: () => void
  handleSseEvent: (event: SseEvent) => void
  submitTurn: (prompt: string) => void
  sendMessage: (prompt: string) => Promise<void>
  cancelTurn: () => Promise<void>
  interruptTurn: () => Promise<void>
  deleteQueuedMessage: (messageId: string) => Promise<void>
  fetchSessions: () => Promise<void>
  createSession: () => Promise<void>
  switchSession: (id: string) => Promise<void>
  loadOlderTurns: () => Promise<void>
  deleteSession: (id: string) => Promise<void>
}

let latestSessionLoadId = 0
let pendingHistoryHydrationAbort: AbortController | null = null
let pendingHistoryHydrationIdleHandle: IdleHandle | null = null
const sessionTitleAnimationTimers = new Map<string, ReturnType<typeof globalThis.setTimeout>>()
let scheduleIdleWork: IdleScheduler = defaultIdleScheduler.schedule
let cancelIdleWork: IdleCanceller = defaultIdleScheduler.cancel

function clearSessionTitleAnimationTimer(sessionId: string) {
  const timer = sessionTitleAnimationTimers.get(sessionId)
  if (timer == null) return
  globalThis.clearTimeout(timer)
  sessionTitleAnimationTimers.delete(sessionId)
}

function scheduleSessionTitleAnimationTick(sessionId: string) {
  clearSessionTitleAnimationTimer(sessionId)
  const timer = globalThis.setTimeout(() => {
    const state = useChatStore.getState()
    const animation = state.sessionTitleAnimations[sessionId]
    if (!animation?.animating) {
      clearSessionTitleAnimationTimer(sessionId)
      return
    }

    const nextLength = Math.min(
      animation.renderedTitle.length + 1,
      animation.targetTitle.length
    )
    const nextRenderedTitle = animation.targetTitle.slice(0, nextLength)
    const done = nextRenderedTitle === animation.targetTitle

    useChatStore.setState((current) => ({
      sessionTitleAnimations: {
        ...current.sessionTitleAnimations,
        [sessionId]: {
          targetTitle: animation.targetTitle,
          renderedTitle: nextRenderedTitle,
          animating: !done,
        },
      },
    }))

    if (done) {
      clearSessionTitleAnimationTimer(sessionId)
      return
    }
    scheduleSessionTitleAnimationTick(sessionId)
  }, SESSION_TITLE_ANIMATION_TICK_MS)
  sessionTitleAnimationTimers.set(sessionId, timer)
}

function startSessionTitleAnimation(
  sessionId: string,
  previousTitle: string,
  nextTitle: string
) {
  useChatStore.setState((state) => ({
    sessionTitleAnimations: {
      ...state.sessionTitleAnimations,
      [sessionId]: {
        targetTitle: nextTitle,
        renderedTitle: previousTitle.slice(0, 1),
        animating: true,
      },
    },
  }))
  scheduleSessionTitleAnimationTick(sessionId)
}

function settleSessionTitleAnimation(sessionId: string, title: string) {
  clearSessionTitleAnimationTimer(sessionId)
  useChatStore.setState((state) => ({
    sessionTitleAnimations: {
      ...state.sessionTitleAnimations,
      [sessionId]: {
        targetTitle: title,
        renderedTitle: title,
        animating: false,
      },
    },
  }))
}

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
    const currentState = get()
    const cachedSnapshot = getSessionSnapshot(id) ?? EMPTY_SESSION_SNAPSHOT
    const historyPagePromise = fetchHistory({
      sessionId: id,
      limit: INITIAL_SESSION_HISTORY_PAGE_SIZE,
    })
    const currentTurnPromise = fetchCurrentTurn(id)
    const sessionInfoPromise = fetchSessionInfo(id)
    const queuePromise = apiFetchQueue(id)
    setActiveWorkspaceRoot(null)

    cacheSessionSnapshot(
      currentState.activeSessionId,
      snapshotFromState(currentState),
      currentState.sessions
    )

    set({
      activeSessionId: id,
      ...applySessionSnapshot(cachedSnapshot, true),
    })

    sessionInfoPromise
      .then((info) => {
        if (loadId !== latestSessionLoadId) return
        setActiveWorkspaceRoot(info.workspace_root)
        cacheSessionSnapshot(
          id,
          {
            ...(getSessionSnapshot(id) ?? cachedSnapshot),
            contextPressure: info.pressure_ratio,
          },
          get().sessions
        )
        set({ contextPressure: info.pressure_ratio })
      })
      .catch(() => {})

    queuePromise
      .then((queueData) => {
        if (loadId !== latestSessionLoadId) return
        cacheSessionSnapshot(
          id,
          {
            ...(getSessionSnapshot(id) ?? cachedSnapshot),
            messageQueue: queueData.messages,
          },
          get().sessions
        )
        set({ messageQueue: queueData.messages })
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
        messageQueue: get().messageQueue,
      }

      cacheSessionSnapshot(id, hydratedSnapshot, get().sessions)

      set({
        ...applySessionSnapshot(hydratedSnapshot, false),
        historyHasMore: historyPage.has_more,
        historyNextBeforeTurnId: historyPage.next_before_turn_id,
      })

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
                messageQueue: get().messageQueue,
              }
              cacheSessionSnapshot(id, nextSnapshot, get().sessions)
              set({
                turns,
                historyHasMore: olderHistoryPage.has_more,
                historyNextBeforeTurnId: olderHistoryPage.next_before_turn_id,
              })
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
      if (currentTurn.status === "waiting_for_question") {
        void usePendingQuestionStore.getState().hydrateForSession(sessionId)
      }
      set((state) => {
        const nextStreamingTurn = currentTurnToStreamingTurn(currentTurn)
        updateSessionSnapshot(
          sessionId,
          (snapshot) => ({
            ...snapshot,
            streamingTurn: nextStreamingTurn,
            chatState: "active",
          }),
          state.sessions
        )
        return {
          chatState: "active",
          streamingTurn: nextStreamingTurn,
        }
      })
    } catch (error) {
      void error
    }
  }

  function refreshActiveSessionPressure(sessionId: string | null) {
    fetchSessionInfo(sessionId ?? undefined)
      .then((info) => {
        const snapshot = getSessionSnapshot(sessionId)
        if (sessionId && snapshot) {
          cacheSessionSnapshot(
            sessionId,
            {
              ...snapshot,
              contextPressure: info.pressure_ratio,
            },
            get().sessions
          )
        }
        set({ contextPressure: info.pressure_ratio })
      })
      .catch(() => {})
  }

  return {
    sessions: [],
    sessionTitleAnimations: {},
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
    messageQueue: [],
    _pendingPrompt: null,

    initialize: () => {
      apiFetchSessions()
        .then((sessions) => {
          const lastActiveId = localStorage.getItem("lastActiveSessionId")
          const activeId =
            (lastActiveId && sessions.some((s) => s.id === lastActiveId))
              ? lastActiveId
              : sessions[0]?.id ?? null
          set({
            sessions,
            sessionTitleAnimations: Object.fromEntries(
              sessions.map((session) => [
                session.id,
                {
                  targetTitle: session.title,
                  renderedTitle: session.title,
                  animating: false,
                },
              ])
            ),
            activeSessionId: activeId,
          })
          if (activeId) {
            localStorage.setItem("lastActiveSessionId", activeId)
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
          updateSessionSnapshot(
            activeId,
            (snapshot) => ({
              ...snapshot,
              streamingTurn: nextStreamingTurn,
            }),
            state.sessions
          )
          return {
            streamingTurn: nextStreamingTurn,
          }
        })
      }

      switch (event.type) {
        case "current_turn_started": {
          if (event.data.session_id !== activeId) break
          const nextStreamingTurn = currentTurnToStreamingTurn(event.data)
          set((state) => {
            updateSessionSnapshot(
              activeId,
              (snapshot) => ({
                ...snapshot,
                streamingTurn: nextStreamingTurn,
                chatState: "active",
              }),
              state.sessions
            )

            return {
              _pendingPrompt: null,
              chatState: "active",
              streamingTurn: nextStreamingTurn,
            }
          })
          break
        }
        case "status": {
          if (event.data.session_id !== activeId) break

          const status = event.data.status as TurnStatus
          if (status === "waiting_for_question" && activeId) {
            void usePendingQuestionStore.getState().hydrateForSession(activeId)
          }
          if (status === "waiting") {
            const prev = get().streamingTurn
            if (prev) {
              set((state) => {
                const nextStreamingTurn = withStreamingStatus(prev, "waiting")
                updateSessionSnapshot(
                  activeId,
                  (snapshot) => ({
                    ...snapshot,
                    streamingTurn: nextStreamingTurn,
                    chatState: "active",
                  }),
                  state.sessions
                )

                return {
                  _pendingPrompt: null,
                  chatState: "active",
                  streamingTurn: nextStreamingTurn,
                }
              })
            } else {
              const prompt = get()._pendingPrompt
              if (prompt) {
                set((state) => {
                  const nextStreamingTurn = createPendingStreamingTurn([prompt])
                  updateSessionSnapshot(
                    activeId,
                    (snapshot) => ({
                      ...snapshot,
                      streamingTurn: nextStreamingTurn,
                      chatState: "active",
                    }),
                    state.sessions
                  )

                  return {
                    _pendingPrompt: null,
                    chatState: "active",
                    streamingTurn: nextStreamingTurn,
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
                updateSessionSnapshot(
                  activeId,
                  (snapshot) => ({
                    ...snapshot,
                    streamingTurn: nextStreamingTurn,
                  }),
                  state.sessions
                )

                return {
                  streamingTurn: nextStreamingTurn,
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
              messageQueue: state.messageQueue,
            }
            cacheSessionSnapshot(activeId, snapshot, state.sessions)
            return {
              turns,
              streamingTurn: null,
              chatState: isWaitingForQuestion ? "active" : "idle",
              error: null,
              lastCompression: null,
            }
          })
          void usePendingQuestionStore.getState().hydrateForSession(activeId)
          refreshActiveSessionPressure(activeId)
          break
        }
        case "context_compressed": {
          if (event.data.session_id !== activeId) break
          set((state) => {
            updateSessionSnapshot(
              activeId,
              (snapshot) => ({
                ...snapshot,
                lastCompression: event.data,
              }),
              state.sessions
            )
            return {
              lastCompression: event.data,
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
          if (event.data.session_id && event.data.session_id !== activeId) break
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
              updateSessionSnapshot(
                activeId,
                (snapshot) => ({
                  ...snapshot,
                  streamingTurn: nextStreamingTurn,
                  chatState: "idle",
                }),
                state.sessions
              )
              return {
                streamingTurn: nextStreamingTurn,
                chatState: "idle",
                error: null,
              }
            })
            break
          }
          set((state) => {
            updateSessionSnapshot(
              activeId,
              (snapshot) => ({
                ...snapshot,
                streamingTurn: null,
                chatState: "idle",
              }),
              state.sessions
            )
            return {
              error: event.data.message,
              streamingTurn: null,
              chatState: "idle",
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
            updateSessionSnapshot(
              activeId,
              (snapshot) => ({
                ...snapshot,
                streamingTurn: nextStreamingTurn,
                chatState: "idle",
              }),
              state.sessions
            )
            return {
              streamingTurn: nextStreamingTurn,
              chatState: "idle",
              error: null,
            }
          })
          break
        }
        case "session_created": {
          get().fetchSessions()
          break
        }
        case "session_updated": {
          const updatedId = event.data.session_id
          const previous = get().sessions.find(
            (session) => session.id === updatedId
          )
          const titleChanged =
            previous?.title != null && previous.title !== event.data.title
          set((state) => ({
            sessions: state.sessions.map((session) =>
              session.id === updatedId
                ? {
                    ...session,
                    title: event.data.title,
                    title_source: event.data.title_source,
                    auto_rename_policy: event.data.auto_rename_policy,
                    updated_at: event.data.updated_at,
                    last_active_at: event.data.last_active_at,
                    model: event.data.model,
                  }
                : session
            ),
          }))
          if (titleChanged && previous) {
            startSessionTitleAnimation(
              updatedId,
              previous.title,
              event.data.title
            )
          } else {
            settleSessionTitleAnimation(updatedId, event.data.title)
          }
          break
        }
        case "session_deleted": {
          const deletedId = event.data.session_id
          const wasActive = get().activeSessionId === deletedId
          let nextActiveId: string | null = null
          clearSessionTitleAnimationTimer(deletedId)

          set((state) => {
            const sessions = state.sessions.filter((s) => s.id !== deletedId)
            const { [deletedId]: _deletedAnimation, ...remainingAnimations } =
              state.sessionTitleAnimations
            trimSessionSnapshotCache(sessions)

            if (!wasActive) {
              return {
                sessions,
                sessionTitleAnimations: remainingAnimations,
              }
            }

            nextActiveId = sessions[0]?.id ?? null
            const nextSnapshot =
              getSessionSnapshot(nextActiveId) ?? EMPTY_SESSION_SNAPSHOT

            return {
              sessions,
              activeSessionId: nextActiveId,
              ...applySessionSnapshot(nextSnapshot, nextActiveId != null),
              historyLoadingMore: false,
              _pendingPrompt: null,
              sessionTitleAnimations: remainingAnimations,
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
        case "message_queued": {
          if (event.data.session_id !== activeId) break
          const newMessage: QueuedMessage = {
            id: event.data.message_id,
            content: event.data.content_preview,
            queued_at_ms: Date.now(),
          }
          set((state) => {
            const nextQueue = [...state.messageQueue, newMessage]
            updateSessionSnapshot(
              activeId,
              (snapshot) => ({
                ...snapshot,
                messageQueue: nextQueue,
              }),
              state.sessions
            )
            return {
              messageQueue: nextQueue,
            }
          })
          break
        }
        case "message_deleted": {
          if (event.data.session_id !== activeId) break
          const deletedId = event.data.message_id
          set((state) => {
            const filteredQueue = state.messageQueue.filter((m) => m.id !== deletedId)
            updateSessionSnapshot(
              activeId,
              (snapshot) => ({
                ...snapshot,
                messageQueue: filteredQueue,
              }),
              state.sessions
            )
            return {
              messageQueue: filteredQueue,
            }
          })
          break
        }
        case "turn_interrupted": {
          if (event.data.session_id !== activeId) break
          const prev = get().streamingTurn
          if (!prev) break
          set((state) => {
            const nextStreamingTurn = { ...prev, status: "cancelled" as const }
            updateSessionSnapshot(
              activeId,
              (snapshot) => ({
                ...snapshot,
                streamingTurn: nextStreamingTurn,
                chatState: "idle",
              }),
              state.sessions
            )
            return {
              streamingTurn: nextStreamingTurn,
              chatState: "idle",
              error: null,
            }
          })
          break
        }
        case "queue_processing": {
          if (event.data.session_id !== activeId) break
          // 队列开始处理时清空显示
          set((state) => {
            updateSessionSnapshot(
              activeId,
              (snapshot) => ({
                ...snapshot,
                messageQueue: [],
              }),
              state.sessions
            )
            return {
              messageQueue: [],
            }
          })
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
          userMessages: [prompt],
          status: "waiting",
          blocks: [],
        }
        const snapshot = getSessionSnapshot(sessionId) ?? EMPTY_SESSION_SNAPSHOT
        const nextState = {
          ...state,
          error: null,
          _pendingPrompt: prompt,
          chatState: "active" as const,
          lastCompression: null,
          streamingTurn: nextStreamingTurn,
        }
        cacheSessionSnapshot(
          sessionId,
          {
            ...snapshotFromState(nextState),
            latestTurn: snapshot.latestTurn,
          },
          state.sessions
        )
        return {
          error: null,
          _pendingPrompt: prompt,
          chatState: "active",
          lastCompression: null,
          streamingTurn: nextStreamingTurn,
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
          cacheSessionSnapshot(
            sessionId,
            snapshotFromState(nextState),
            state.sessions
          )
          return {
            error: err instanceof Error ? err.message : "Network error",
            _pendingPrompt: null,
            streamingTurn: null,
            chatState: "idle",
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
          cacheSessionSnapshot(
            sessionId,
            snapshotFromState(nextState),
            state.sessions
          )
          return {
            streamingTurn: nextStreamingTurn,
            chatState: "idle",
            error: null,
          }
        })
      } catch (err) {
        set({
          error: err instanceof Error ? err.message : "Cancel failed",
        })
      }
    },

    sendMessage: async (prompt: string) => {
      const sessionId = get().activeSessionId
      if (!sessionId) return

      const chatState = get().chatState
      if (chatState === "idle") {
        // 空闲时立即开始 turn
        get().submitTurn(prompt)
        return
      }

      // 运行时入队
      try {
        const result = await apiSendMessage(prompt, sessionId)
        if (result.status === "started") {
          // 立即开始了，不需要额外处理
        }
        // 如果是 queued，SSE 事件会更新队列
      } catch (err) {
        set({
          error: err instanceof Error ? err.message : "Send message failed",
        })
      }
    },

    interruptTurn: async () => {
      const sessionId = get().activeSessionId
      const streamingTurn = get().streamingTurn
      if (!sessionId || !streamingTurn) return

      try {
        await apiInterruptTurn(sessionId)
        // SSE 事件会处理状态更新
      } catch (err) {
        set({
          error: err instanceof Error ? err.message : "Interrupt failed",
        })
      }
    },

    deleteQueuedMessage: async (messageId: string) => {
      const sessionId = get().activeSessionId
      if (!sessionId) return

      try {
        await apiDeleteQueuedMessage(messageId, sessionId)
        // SSE 事件会更新队列
      } catch (err) {
        set({
          error: err instanceof Error ? err.message : "Delete message failed",
        })
      }
    },

    fetchSessions: async () => {
      const sessions = await apiFetchSessions()
      trimSessionSnapshotCache(sessions)
      set((state) => ({
        sessions,
        sessionTitleAnimations: Object.fromEntries(
          sessions.map((session) => {
            const existing = state.sessionTitleAnimations[session.id]
            return [
              session.id,
              existing
                ? {
                    targetTitle: session.title,
                    renderedTitle: existing.animating
                      ? existing.renderedTitle
                      : session.title,
                    animating:
                      existing.animating &&
                      existing.targetTitle === session.title,
                  }
                : {
                    targetTitle: session.title,
                    renderedTitle: session.title,
                    animating: false,
                  },
            ]
          })
        ),
      }))
    },

    createSession: async () => {
      const session = await apiCreateSession()
      set((state) => {
        const nextSessions = [...state.sessions, session]
        trimSessionSnapshotCache(nextSessions)
        return {
          sessions: nextSessions,
          sessionTitleAnimations: {
            ...state.sessionTitleAnimations,
            [session.id]: {
              targetTitle: session.title,
              renderedTitle: session.title,
              animating: false,
            },
          },
        }
      })
      await get().switchSession(session.id)
    },

    switchSession: async (id: string) => {
      if (id === get().activeSessionId && !get().sessionHydrating) {
        return
      }
      cancelPendingHistoryHydration()
      localStorage.setItem("lastActiveSessionId", id)
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
          updateSessionSnapshot(
            sessionId,
            (snapshot) => ({
              ...snapshot,
              latestTurn: latestTurn(turns),
            }),
            state.sessions
          )
          return {
            turns,
            historyHasMore: historyPage.has_more,
            historyNextBeforeTurnId: historyPage.next_before_turn_id,
            historyLoadingMore: false,
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
      clearSessionTitleAnimationTimer(id)
      const state = get()
      const remaining = state.sessions.filter((s) => s.id !== id)
      const nextAnimations = { ...state.sessionTitleAnimations }
      removeSessionSnapshot(id)
      trimSessionSnapshotCache(remaining)
      delete nextAnimations[id]
      set({
        sessions: remaining,
        sessionTitleAnimations: nextAnimations,
      })

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
