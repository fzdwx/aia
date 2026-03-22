import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react"
import { ArrowDown } from "lucide-react"
import {
  CompressionNotice,
  MemoizedStreamingView,
  MemoizedTurnView,
  SessionHydratingIndicator,
  StatusIndicator,
} from "@/features/chat/message-sections"
import {
  distanceFromBottom,
  HISTORY_LOAD_TRIGGER_PX,
  shouldShowHistoryHint,
  shouldStickToBottom,
} from "@/components/chat-messages-helpers"
import { useChatStore } from "@/stores/chat-store"

function usePrefersReducedMotion() {
  const [prefersReducedMotion, setPrefersReducedMotion] = useState(false)

  useEffect(() => {
    if (
      typeof window === "undefined" ||
      typeof window.matchMedia !== "function"
    ) {
      return
    }

    const mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)")
    const updatePreference = () => {
      setPrefersReducedMotion(mediaQuery.matches)
    }

    updatePreference()
    mediaQuery.addEventListener("change", updatePreference)

    return () => {
      mediaQuery.removeEventListener("change", updatePreference)
    }
  }, [])

  return prefersReducedMotion
}

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
  const bottomAnchorRef = useRef<HTMLDivElement>(null)
  const historyTriggerRef = useRef<HTMLDivElement>(null)
  const previousSessionIdRef = useRef<string | null>(null)
  const previousTurnCountRef = useRef(0)
  const previousStreamingBlockCountRef = useRef(0)
  const shouldStickToBottomRef = useRef(true)
  const skipNextAutoScrollRef = useRef(false)
  const isStreamingRef = useRef(false)
  const autoLoadingOlderTurnsRef = useRef(false)
  const historyHasMoreRef = useRef(historyHasMore)
  const historyLoadingMoreRef = useRef(historyLoadingMore)
  const sessionHydratingRef = useRef(sessionHydrating)
  const rafPendingRef = useRef(false)
  const [scrollTop, setScrollTop] = useState(0)
  const [isAtBottom, setIsAtBottom] = useState(true)

  const showHistoryHint = shouldShowHistoryHint(historyLoadingMore, scrollTop)
  const isStreaming = !!streamingTurn
  isStreamingRef.current = isStreaming

  const scrollToBottom = useCallback(() => {
    bottomAnchorRef.current?.scrollIntoView({ behavior: "auto" })
    shouldStickToBottomRef.current = true
    setIsAtBottom(true)
  }, [])

  const handleLoadOlderTurns = useCallback(async () => {
    if (
      autoLoadingOlderTurnsRef.current ||
      historyLoadingMoreRef.current ||
      sessionHydratingRef.current ||
      !historyHasMoreRef.current
    ) {
      return
    }

    autoLoadingOlderTurnsRef.current = true
    shouldStickToBottomRef.current = false
    const container = containerRef.current
    const previousScrollHeight = container?.scrollHeight ?? 0
    skipNextAutoScrollRef.current = true
    try {
      await loadOlderTurns()
      requestAnimationFrame(() => {
        const nextContainer = containerRef.current
        if (!nextContainer) {
          autoLoadingOlderTurnsRef.current = false
          return
        }
        const nextScrollHeight = nextContainer.scrollHeight
        nextContainer.scrollTop += nextScrollHeight - previousScrollHeight
        setScrollTop(nextContainer.scrollTop)
        autoLoadingOlderTurnsRef.current = false
      })
    } catch {
      autoLoadingOlderTurnsRef.current = false
    }
  }, [loadOlderTurns])

  useEffect(() => {
    historyHasMoreRef.current = historyHasMore
    historyLoadingMoreRef.current = historyLoadingMore
    sessionHydratingRef.current = sessionHydrating
  }, [historyHasMore, historyLoadingMore, sessionHydrating])

  // Scroll event tracking + container resize observer
  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const handleScroll = () => {
      const dist = distanceFromBottom({
        scrollHeight: container.scrollHeight,
        scrollTop: container.scrollTop,
        clientHeight: container.clientHeight,
      })
      shouldStickToBottomRef.current = shouldStickToBottom(dist)

      if (!rafPendingRef.current) {
        rafPendingRef.current = true
        requestAnimationFrame(() => {
          rafPendingRef.current = false
          setScrollTop(container.scrollTop)
          setIsAtBottom(shouldStickToBottomRef.current)
        })
      }

      if (typeof IntersectionObserver === "undefined") {
        if (container.scrollTop <= HISTORY_LOAD_TRIGGER_PX) {
          void handleLoadOlderTurns()
        }
      }
    }

    const resizeObserver = new ResizeObserver(() => {
      handleScroll()
    })

    handleScroll()
    container.addEventListener("scroll", handleScroll)
    resizeObserver.observe(container)
    return () => {
      container.removeEventListener("scroll", handleScroll)
      resizeObserver.disconnect()
    }
  }, [activeSessionId, handleLoadOlderTurns])

  // Content resize observer — auto-scroll when content grows during streaming
  const hasContent = turns.length > 0 || isStreaming
  useEffect(() => {
    const content = contentRef.current
    if (!content) return

    const resizeObserver = new ResizeObserver(() => {
      if (isStreamingRef.current && shouldStickToBottomRef.current) {
        bottomAnchorRef.current?.scrollIntoView({ behavior: "auto" })
      }
    })

    resizeObserver.observe(content)
    return () => {
      resizeObserver.disconnect()
    }
  }, [activeSessionId, hasContent])

  // Intersection observer for history loading trigger
  useEffect(() => {
    const container = containerRef.current
    const historyTrigger = historyTriggerRef.current
    if (!container || !historyTrigger) {
      return
    }

    if (typeof IntersectionObserver === "undefined") {
      if (container.scrollTop <= HISTORY_LOAD_TRIGGER_PX) {
        void handleLoadOlderTurns()
      }
      return
    }

    const observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[0]
        if (!entry?.isIntersecting) return
        void handleLoadOlderTurns()
      },
      {
        root: container,
        rootMargin: "80px 0px 0px 0px",
      }
    )

    observer.observe(historyTrigger)
    return () => {
      observer.disconnect()
    }
  }, [
    activeSessionId,
    turns.length,
    historyHasMore,
    historyLoadingMore,
    sessionHydrating,
    handleLoadOlderTurns,
  ])

  // Auto-scroll on session change, batch hydration, and new streaming blocks
  useLayoutEffect(() => {
    const container = containerRef.current
    if (!container) return

    const currentStreamingBlockCount = streamingTurn?.blocks.length ?? 0
    const sessionChanged = previousSessionIdRef.current !== activeSessionId

    if (sessionChanged) {
      shouldStickToBottomRef.current = true
      skipNextAutoScrollRef.current = false
      container.scrollTop = container.scrollHeight
      setScrollTop(container.scrollTop)
      setIsAtBottom(true)
      previousSessionIdRef.current = activeSessionId
      previousTurnCountRef.current = turns.length
      previousStreamingBlockCountRef.current = currentStreamingBlockCount
      return
    }

    if (skipNextAutoScrollRef.current) {
      skipNextAutoScrollRef.current = false
      previousSessionIdRef.current = activeSessionId
      previousTurnCountRef.current = turns.length
      previousStreamingBlockCountRef.current = currentStreamingBlockCount
      return
    }

    const hydratedManyTurns = turns.length > previousTurnCountRef.current + 1
    const hydratedStreamingSnapshot =
      currentStreamingBlockCount > previousStreamingBlockCountRef.current + 1

    const shouldAutoScroll =
      hydratedManyTurns ||
      hydratedStreamingSnapshot ||
      shouldStickToBottomRef.current

    if (!shouldAutoScroll) {
      previousSessionIdRef.current = activeSessionId
      previousTurnCountRef.current = turns.length
      previousStreamingBlockCountRef.current = currentStreamingBlockCount
      return
    }

    const behavior: ScrollBehavior =
      prefersReducedMotion ||
      hydratedManyTurns ||
      hydratedStreamingSnapshot ||
      isStreaming
        ? "auto"
        : "smooth"

    bottomAnchorRef.current?.scrollIntoView({ behavior })

    previousSessionIdRef.current = activeSessionId
    previousTurnCountRef.current = turns.length
    previousStreamingBlockCountRef.current = currentStreamingBlockCount
  }, [
    activeSessionId,
    turns.length,
    streamingTurn?.blocks.length,
    prefersReducedMotion,
    isStreaming,
  ])

  if (turns.length === 0 && !streamingTurn) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center px-4">
        <h2 className="font-serif text-3xl font-semibold tracking-tight text-foreground">
          What can I help with?
        </h2>
        <p className="mt-2.5 text-sm text-muted-foreground">
          Start a conversation or ask anything.
        </p>
        {error && (
          <p className="mt-4 max-w-md text-center text-sm text-destructive">
            {error}
          </p>
        )}
      </div>
    )
  }

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={containerRef}
        className="h-full overflow-y-auto"
        role="log"
        aria-live="polite"
        aria-relevant="additions text"
        aria-busy={sessionHydrating}
      >
        <div className="mx-auto max-w-[720px] px-4 py-6 sm:px-6 sm:py-8">
          {sessionHydrating && (
            <SessionHydratingIndicator reducedMotion={prefersReducedMotion} />
          )}
          {historyHasMore && (
            <>
              <div
                ref={historyTriggerRef}
                className="h-px w-full"
                aria-hidden="true"
              />
              <div
                className={
                  showHistoryHint
                    ? "pointer-events-none sticky top-0 z-10 -mx-4 mb-4 flex justify-center bg-gradient-to-b from-background via-background/94 to-transparent px-4 pt-2 pb-3 opacity-100 transition-opacity duration-150 sm:-mx-6 sm:px-6"
                    : "pointer-events-none sticky top-0 z-10 -mx-4 mb-4 flex justify-center bg-gradient-to-b from-background via-background/94 to-transparent px-4 pt-2 pb-3 opacity-0 transition-opacity duration-150 sm:-mx-6 sm:px-6"
                }
                aria-hidden={!showHistoryHint}
              >
                <div
                  className="max-w-full rounded-full border border-border/35 bg-background/88 px-3 py-1 text-center text-xs text-muted-foreground/80 shadow-none"
                  role={historyLoadingMore ? "status" : undefined}
                  aria-live={historyLoadingMore ? "polite" : undefined}
                  aria-atomic={historyLoadingMore ? "true" : undefined}
                >
                  {historyLoadingMore
                    ? "Loading older messages…"
                    : "Scroll up for older messages"}
                </div>
              </div>
            </>
          )}
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
            {lastCompression && !streamingTurn && (
              <CompressionNotice summary={lastCompression.summary} />
            )}
            {streamingTurn && (
              <MemoizedStreamingView streaming={streamingTurn} />
            )}
            {error && (
              <div className="mb-4 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs leading-relaxed font-medium text-destructive">
                {error}
              </div>
            )}
            <div ref={bottomAnchorRef} aria-hidden="true" />
          </div>
        </div>
        {streamingTurn && (
          <div className="sticky bottom-0 z-10 bg-gradient-to-t from-background via-background to-transparent pt-6 pb-4">
            <div className="mx-auto max-w-[720px] px-4 sm:px-6">
              <StatusIndicator status={streamingTurn.status} />
            </div>
          </div>
        )}
      </div>
      <button
        type="button"
        aria-label="Scroll to bottom"
        className={
          isAtBottom
            ? "pointer-events-none absolute right-4 bottom-4 z-20 flex size-8 items-center justify-center rounded-full border border-border/50 bg-background/90 text-muted-foreground opacity-0 shadow-sm backdrop-blur-sm transition-opacity duration-200"
            : "absolute right-4 bottom-4 z-20 flex size-8 items-center justify-center rounded-full border border-border/50 bg-background/90 text-muted-foreground opacity-100 shadow-sm backdrop-blur-sm transition-opacity duration-200 hover:bg-accent hover:text-accent-foreground"
        }
        tabIndex={isAtBottom ? -1 : 0}
        onClick={scrollToBottom}
      >
        <ArrowDown className="size-4" />
      </button>
    </div>
  )
}
