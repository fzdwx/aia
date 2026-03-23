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
  shouldLoadOlderTurnsOnScroll,
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

  // Effect 1: Scroll handler — detect direction, RAF-throttled state updates
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

  // Effect 2: Content ResizeObserver — auto-scroll when content grows
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

  // Effect 3: Session change — reset autoFollow and scroll to bottom
  useLayoutEffect(() => {
    if (prevSessionIdRef.current === activeSessionId) return
    prevSessionIdRef.current = activeSessionId
    autoFollowRef.current = true
    scrollAnchorRef.current = null
    scrollToBottom()
  }, [activeSessionId, scrollToBottom])

  // Effect 4: when the list mounts or new bottom-follow content appears, snap to bottom.
  useLayoutEffect(() => {
    if (!activeSessionId || !autoFollowRef.current) return
    if (turns.length === 0 && !streamingTurn) return
    scrollToBottom()
  }, [
    activeSessionId,
    turns.length,
    streamingTurn,
    scrollToBottom,
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
        className="h-full overflow-y-auto [overflow-anchor:none]"
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
            <div aria-hidden="true" className="h-px [overflow-anchor:auto]" />
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
