import { useEffect, useState } from "react"

export const ACTIVE_DURATION_TICK_MS = 100

export function useDurationTicker(enabled: boolean) {
  const [, setTick] = useState(0)

  useEffect(() => {
    if (!enabled) return

    const timer = window.setInterval(() => {
      setTick((current) => current + 1)
    }, ACTIVE_DURATION_TICK_MS)

    return () => window.clearInterval(timer)
  }, [enabled])
}
