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
  const prevScrollTopRef = useRef(-1)
  const rafPendingRef = useRef(false)
  const scrollAnchorRef = useRef<{
    scrollHeight: number
    scrollTop: number
  } | null>(null)
  const handleLoadOlderTurnsRef = useRef<() => Promise<void>>(
    () => Promise.resolve()
  )
  const smoothScrollingRef = useRef(false)

  const [scrollTop, setScrollTop] = useState(0)
  const [isAtBottom, setIsAtBottom] = useState(true)

  const showHistoryHint = shouldShowHistoryHint(historyLoadingMore, scrollTop)

  const hasContent = turns.length > 0 || !!streamingTurn

  const scrollToBottomInstant = useCallback(() => {
    const container = containerRef.current
    if (!container) return
    autoFollowRef.current = true
    container.scrollTop = container.scrollHeight
    prevScrollTopRef.current = container.scrollTop
    setIsAtBottom(true)
  }, [])

  const scrollToBottomSmooth = useCallback(() => {
    const container = containerRef.current
    if (!container) return
    autoFollowRef.current = true
    smoothScrollingRef.current = true
    const prefersReduced = window.matchMedia(
      "(prefers-reduced-motion: reduce)"
    ).matches
    if (prefersReduced) {
      container.scrollTop = container.scrollHeight
      prevScrollTopRef.current = container.scrollTop
      smoothScrollingRef.current = false
    } else {
      container.scrollTo({ top: container.scrollHeight, behavior: "smooth" })
    }
    setIsAtBottom(true)
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

  // 保持 ref 同步，scroll listener 通过 ref 调用，不需要重建
  useEffect(() => {
    handleLoadOlderTurnsRef.current = handleLoadOlderTurns
  }, [handleLoadOlderTurns])

  // Scroll event handler
  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    prevScrollTopRef.current = container.scrollTop

    const handleScroll = () => {
      const dist = distanceFromBottom({
        scrollHeight: container.scrollHeight,
        scrollTop: container.scrollTop,
        clientHeight: container.clientHeight,
      })
      const nextIsAtBottom = shouldStickToBottom(dist)

      const userScrolledUp =
        prevScrollTopRef.current >= 0
          ? container.scrollTop < prevScrollTopRef.current
          : false

      if (userScrolledUp) {
        autoFollowRef.current = false
      } else if (nextIsAtBottom) {
        autoFollowRef.current = true
        smoothScrollingRef.current = false
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
        void handleLoadOlderTurnsRef.current()
      }
    }

    handleScroll()
    container.addEventListener("scroll", handleScroll)
    return () => {
      container.removeEventListener("scroll", handleScroll)
    }
  }, [activeSessionId, hasContent])

  // Auto-follow 核心：ResizeObserver 监听内容尺寸变化
  useEffect(() => {
    const content = contentRef.current
    if (!content) return

    const resizeObserver = new ResizeObserver(() => {
      const container = containerRef.current
      if (!container) return

      if (autoFollowRef.current) {
        container.scrollTop = container.scrollHeight
        prevScrollTopRef.current = container.scrollTop
      }
    })

    resizeObserver.observe(content)
    // ResizeObserver 不会在初始 observe 时触发回调，
    // 但容器重建时（如 empty→有内容）需要立即滚到底部
    if (autoFollowRef.current) {
      const container = containerRef.current
      if (container) {
        container.scrollTop = container.scrollHeight
        prevScrollTopRef.current = container.scrollTop
      }
    }
    return () => {
      resizeObserver.disconnect()
    }
  }, [activeSessionId, hasContent])

  // 加载历史消息后恢复滚动位置
  useLayoutEffect(() => {
    const container = containerRef.current
    if (!container) return

    const anchor = scrollAnchorRef.current
    if (anchor !== null) {
      scrollAnchorRef.current = null
      const restore = () => {
        const currentContainer = containerRef.current
        if (!currentContainer) return
        const added = currentContainer.scrollHeight - anchor.scrollHeight
        if (added > 0) {
          currentContainer.scrollTop = anchor.scrollTop + added
          prevScrollTopRef.current = currentContainer.scrollTop
        }
      }
      requestAnimationFrame(() => {
        requestAnimationFrame(restore)
      })
    }
  }, [turns.length])

  // 切换 session：强制滚到底部
  useLayoutEffect(() => {
    if (prevSessionIdRef.current === activeSessionId) return
    prevSessionIdRef.current = activeSessionId
    autoFollowRef.current = true
    scrollAnchorRef.current = null
    prevScrollTopRef.current = -1
    scrollToBottomInstant()
  }, [activeSessionId, scrollToBottomInstant])

  // 用户发消息后强制滚到底部
  // streamingTurn.userMessages 在 submitTurn 时立即设置，比 SSE 事件更早
  const userMessageCount = streamingTurn?.userMessages?.length ?? 0
  const prevUserMessageCountRef = useRef(0)
  useLayoutEffect(() => {
    if (userMessageCount > prevUserMessageCountRef.current) {
      autoFollowRef.current = true
      scrollToBottomInstant()
    }
    prevUserMessageCountRef.current = userMessageCount
  }, [userMessageCount, scrollToBottomInstant])

  if (!hasContent) {
    return <ChatMessagesEmptyState error={error} />
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div
        ref={containerRef}
        className="min-h-0 flex-1 overflow-y-auto [overflow-anchor:none]"
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
      <ScrollToBottomButton
        isAtBottom={isAtBottom}
        onClick={scrollToBottomSmooth}
      />
    </div>
  )
}
