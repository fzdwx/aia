import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react"

import {
  CompressionNotice,
  MemoizedStreamingView,
  MemoizedTurnView,
  SessionHydratingIndicator,
  StatusIndicator,
} from "@/features/chat/message-sections"
import {
  distanceFromBottom,
  shouldLoadOlderTurnsOnScroll,
  shouldResumeAutoFollow,
  shouldShowHistoryHint,
  shouldStickToBottom,
} from "./helpers"
import { useChatStore } from "@/stores/chat-store"

import { ChatMessagesEmptyState } from "./chat-messages-empty-state"
import { ChatMessagesHistoryHint } from "./chat-messages-history-hint"
import { ScrollToBottomButton } from "./scroll-to-bottom-button"
import { usePrefersReducedMotion } from "./use-prefers-reduced-motion"

export function ChatMessages() {
  const turns = useChatStore((s) => s.turns)
  const sessionHydrating = useChatStore((s) => s.sessionHydrating)
  const historyHasMore = useChatStore((s) => s.historyHasMore)
  const historyLoadingMore = useChatStore((s) => s.historyLoadingMore)
  const loadOlderTurns = useChatStore((s) => s.loadOlderTurns)
  const streamingTurn = useChatStore((s) => s.streamingTurn)
  const error = useChatStore((s) => s.error)
  const lastCompression = useChatStore((s) => s.lastCompression)
  const activeSessionId = useChatStore((s) => s.activeSessionId)
  const prefersReducedMotion = usePrefersReducedMotion()

  const containerRef = useRef<HTMLDivElement>(null)
  const contentRef = useRef<HTMLDivElement>(null)
  const autoFollowRef = useRef(true)
  const prevSessionIdRef = useRef<string | null>(null)
  const prevStreamingTurnRef = useRef(streamingTurn)
  const prevScrollTopRef = useRef(0)
  const rafPendingRef = useRef(false)
  const scrollAnchorRef = useRef<{
    scrollHeight: number
    scrollTop: number
  } | null>(null)

  const [scrollTop, setScrollTop] = useState(0)
  const [isAtBottom, setIsAtBottom] = useState(true)

  const showHistoryHint = shouldShowHistoryHint(historyLoadingMore, scrollTop)
  const showEmptyState =
    !sessionHydrating && turns.length === 0 && !streamingTurn

  const scrollToBottom = useCallback(() => {
    const container = containerRef.current
    if (!container) return
    autoFollowRef.current = true
    container.scrollTop = container.scrollHeight
    prevScrollTopRef.current = container.scrollTop
    setIsAtBottom(true)
  }, [])

  const alignToBottom = useCallback(() => {
    const container = containerRef.current
    if (!container) return
    container.scrollTop = container.scrollHeight
    prevScrollTopRef.current = container.scrollTop
    const dist = distanceFromBottom({
      scrollHeight: container.scrollHeight,
      scrollTop: container.scrollTop,
      clientHeight: container.clientHeight,
    })
    setIsAtBottom(shouldStickToBottom(dist))
  }, [])

  const handleLoadOlderTurns = useCallback(async () => {
    if (historyLoadingMore || sessionHydrating || !historyHasMore) return
    const container = containerRef.current
    if (!container) return

    scrollAnchorRef.current = {
      scrollHeight: container.scrollHeight,
      scrollTop: container.scrollTop,
    }
    autoFollowRef.current = false
    await loadOlderTurns()
  }, [loadOlderTurns, historyLoadingMore, sessionHydrating, historyHasMore])

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const handleScroll = () => {
      const dist = distanceFromBottom({
        scrollHeight: container.scrollHeight,
        scrollTop: container.scrollTop,
        clientHeight: container.clientHeight,
      })
      const nextIsAtBottom = shouldStickToBottom(dist)

      const userScrolledUp = container.scrollTop < prevScrollTopRef.current
      if (userScrolledUp) {
        autoFollowRef.current = false
      } else if (shouldResumeAutoFollow(dist)) {
        autoFollowRef.current = true
      }

      prevScrollTopRef.current = container.scrollTop

      if (!rafPendingRef.current) {
        rafPendingRef.current = true
        requestAnimationFrame(() => {
          rafPendingRef.current = false
          setScrollTop(container.scrollTop)
          setIsAtBottom(nextIsAtBottom)
        })
      }

      if (
        shouldLoadOlderTurnsOnScroll({
          scrollTop: container.scrollTop,
          scrollHeight: container.scrollHeight,
          clientHeight: container.clientHeight,
          userScrolledUp,
        })
      ) {
        void handleLoadOlderTurns()
      }
    }

    handleScroll()
    container.addEventListener("scroll", handleScroll)
    return () => {
      container.removeEventListener("scroll", handleScroll)
    }
  }, [activeSessionId, handleLoadOlderTurns])

  useEffect(() => {
    const content = contentRef.current
    if (!content) return

    const resizeObserver = new ResizeObserver(() => {
      const container = containerRef.current
      if (!container) return

      if (autoFollowRef.current) {
        alignToBottom()
      }
    })

    resizeObserver.observe(content)
    return () => {
      resizeObserver.disconnect()
    }
  }, [activeSessionId, alignToBottom])

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const resizeObserver = new ResizeObserver(() => {
      if (autoFollowRef.current) {
        alignToBottom()
      }
    })

    resizeObserver.observe(container)
    return () => {
      resizeObserver.disconnect()
    }
  }, [activeSessionId, alignToBottom])

  useLayoutEffect(() => {
    const container = containerRef.current
    if (!container) return

    const anchor = scrollAnchorRef.current
    if (anchor !== null) {
      scrollAnchorRef.current = null
      // 使用 requestAnimationFrame 确保 DOM 完全更新
      requestAnimationFrame(() => {
        const currentContainer = containerRef.current
        if (!currentContainer) return
        const added = currentContainer.scrollHeight - anchor.scrollHeight
        if (added > 0) {
          currentContainer.scrollTop = anchor.scrollTop + added
        }
      })
    }
  }, [turns.length])

  useLayoutEffect(() => {
    if (prevSessionIdRef.current === activeSessionId) return
    prevSessionIdRef.current = activeSessionId
    autoFollowRef.current = true
    scrollAnchorRef.current = null
    scrollToBottom()
    requestAnimationFrame(() => {
      if (autoFollowRef.current) {
        alignToBottom()
      }
    })
  }, [activeSessionId, alignToBottom, scrollToBottom])

  useLayoutEffect(() => {
    if (!activeSessionId) return

    const previousStreamingTurn = prevStreamingTurnRef.current
    prevStreamingTurnRef.current = streamingTurn

    const startedNewStreamingTurn =
      previousStreamingTurn == null && streamingTurn != null

    if (!startedNewStreamingTurn) return

    autoFollowRef.current = true
    scrollToBottom()
    requestAnimationFrame(() => {
      if (autoFollowRef.current) {
        alignToBottom()
      }
    })
  }, [activeSessionId, streamingTurn, alignToBottom, scrollToBottom])

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={containerRef}
        className="h-full overflow-y-auto [overflow-anchor:none]"
        role="log"
        aria-live="polite"
        aria-relevant="additions text"
        aria-busy={sessionHydrating}
      >
        <div className="mx-auto max-w-[720px] px-4 py-6 sm:px-6 sm:py-8">
          {sessionHydrating ? (
            <SessionHydratingIndicator reducedMotion={prefersReducedMotion} />
          ) : null}
          {historyHasMore ? (
            <ChatMessagesHistoryHint
              historyLoadingMore={historyLoadingMore}
              showHistoryHint={showHistoryHint}
            />
          ) : null}
          <div
            ref={contentRef}
            className={
              sessionHydrating
                ? "opacity-80 transition-opacity duration-150 ease-out"
                : "opacity-100 transition-opacity duration-150 ease-out"
            }
          >
            {showEmptyState ? (
              <ChatMessagesEmptyState error={error} />
            ) : (
              <>
                {turns.map((turn) => (
                  <MemoizedTurnView key={turn.turn_id} turn={turn} />
                ))}
                {lastCompression && !streamingTurn ? (
                  <CompressionNotice summary={lastCompression.summary} />
                ) : null}
                {streamingTurn ? (
                  <MemoizedStreamingView streaming={streamingTurn} />
                ) : null}
                {error ? (
                  <div className="mb-4 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs leading-relaxed font-medium text-destructive">
                    {error}
                  </div>
                ) : null}
              </>
            )}
            <div aria-hidden="true" className="h-px [overflow-anchor:auto]" />
          </div>
        </div>
        {streamingTurn ? (
          <div className="sticky bottom-0 z-10 bg-gradient-to-t from-background via-background to-transparent pt-6 pb-4">
            <div className="mx-auto max-w-[720px] px-4 sm:px-6">
              <StatusIndicator status={streamingTurn.status} />
            </div>
          </div>
        ) : null}
      </div>
      <div className="pointer-events-none absolute inset-x-0 bottom-4 z-20 flex justify-center px-4 sm:bottom-6">
        <div className="pointer-events-auto">
          <ScrollToBottomButton
            isAtBottom={isAtBottom}
            onClick={scrollToBottom}
          />
        </div>
      </div>
    </div>
  )
}
