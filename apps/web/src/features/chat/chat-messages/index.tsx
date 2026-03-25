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
  shouldShowHistoryHint,
  shouldStickToBottom,
} from "@/components/chat-messages-helpers"
import { useChatStore } from "@/stores/chat-store"

import { ChatMessagesEmptyState } from "./chat-messages-empty-state"
import { ChatMessagesHistoryHint } from "./chat-messages-history-hint"
import { ScrollToBottomButton } from "./scroll-to-bottom-button"
import { usePrefersReducedMotion } from "./use-prefers-reduced-motion"

export function ChatMessagesView() {
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
  const prevScrollTopRef = useRef(0)
  const rafPendingRef = useRef(false)
  const scrollAnchorRef = useRef<number | null>(null)

  const [scrollTop, setScrollTop] = useState(0)
  const [isAtBottom, setIsAtBottom] = useState(true)

  const showHistoryHint = shouldShowHistoryHint(historyLoadingMore, scrollTop)

  const scrollToBottom = useCallback(() => {
    const container = containerRef.current
    if (!container) return
    autoFollowRef.current = true
    container.scrollTop = container.scrollHeight
    prevScrollTopRef.current = container.scrollTop
    setIsAtBottom(true)
  }, [])

  const handleLoadOlderTurns = useCallback(async () => {
    if (historyLoadingMore || sessionHydrating || !historyHasMore) return
    const container = containerRef.current
    if (!container) return
    autoFollowRef.current = false
    scrollAnchorRef.current = container.scrollHeight
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
      } else if (nextIsAtBottom) {
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

      const anchor = scrollAnchorRef.current
      if (anchor !== null) {
        scrollAnchorRef.current = null
        const added = container.scrollHeight - anchor
        if (added > 0) container.scrollTop += added
        return
      }

      if (autoFollowRef.current) {
        scrollToBottom()
      }
    })

    resizeObserver.observe(content)
    return () => {
      resizeObserver.disconnect()
    }
  }, [activeSessionId, scrollToBottom])

  useLayoutEffect(() => {
    if (prevSessionIdRef.current === activeSessionId) return
    prevSessionIdRef.current = activeSessionId
    autoFollowRef.current = true
    scrollAnchorRef.current = null
    scrollToBottom()
  }, [activeSessionId, scrollToBottom])

  useLayoutEffect(() => {
    if (!activeSessionId || !autoFollowRef.current) return
    if (turns.length === 0 && !streamingTurn) return
    scrollToBottom()
  }, [activeSessionId, turns.length, streamingTurn, scrollToBottom])

  if (turns.length === 0 && !streamingTurn) {
    return <ChatMessagesEmptyState error={error} />
  }

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
      <ScrollToBottomButton isAtBottom={isAtBottom} onClick={scrollToBottom} />
    </div>
  )
}
