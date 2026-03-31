import {
  startTransition,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react"

import { createIdleScheduler } from "@/lib/idle"

const DIFF_VISIBILITY_ROOT_MARGIN = "240px 0px"

export function LazyDiffMount({ children }: { children: ReactNode }) {
  const [shouldRender, setShouldRender] = useState(false)
  const containerRef = useRef<HTMLDivElement | null>(null)

  useEffect(() => {
    if (shouldRender) return

    const element = containerRef.current
    if (!element) return

    const { schedule, cancel } = createIdleScheduler()
    let cancelScheduledMount: (() => void) | null = null

    const queueMount = () => {
      if (cancelScheduledMount) return
      const handle = schedule(() => {
        startTransition(() => {
          setShouldRender(true)
        })
      })
      cancelScheduledMount = () => cancel(handle)
    }

    if (typeof IntersectionObserver !== "function") {
      queueMount()
      return () => {
        cancelScheduledMount?.()
      }
    }

    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return
        observer.disconnect()
        queueMount()
      },
      { rootMargin: DIFF_VISIBILITY_ROOT_MARGIN }
    )

    observer.observe(element)

    return () => {
      observer.disconnect()
      cancelScheduledMount?.()
    }
  }, [shouldRender])

  return (
    <div
      ref={containerRef}
      className="tool-timeline-lazy-diff"
      data-state={shouldRender ? "ready" : "pending"}
    >
      {shouldRender ? children : null}
    </div>
  )
}
