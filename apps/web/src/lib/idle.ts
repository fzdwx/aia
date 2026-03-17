export type IdleHandle = number

export type IdleScheduler = (callback: () => void) => IdleHandle
export type IdleCanceller = (handle: IdleHandle) => void

type IdleCallbackWithDeadline = (deadline: { timeRemaining(): number }) => void

type BrowserIdleTarget = {
  requestIdleCallback?: (
    callback: IdleCallbackWithDeadline,
    options?: { timeout?: number }
  ) => number
  cancelIdleCallback?: (handle: number) => void
  setTimeout: (handler: TimerHandler, timeout?: number) => number
  clearTimeout: (handle?: number) => void
}

function hasIdleCallbackSupport(
  target: BrowserIdleTarget
): target is BrowserIdleTarget & {
  requestIdleCallback: NonNullable<BrowserWindowWithIdle["requestIdleCallback"]>
  cancelIdleCallback: NonNullable<BrowserWindowWithIdle["cancelIdleCallback"]>
} {
  return (
    typeof (target as BrowserIdleTarget).requestIdleCallback === "function" &&
    typeof (target as BrowserIdleTarget).cancelIdleCallback === "function"
  )
}

export function createIdleScheduler(
  target: BrowserIdleTarget = globalThis as unknown as BrowserIdleTarget
): {
  schedule: IdleScheduler
  cancel: IdleCanceller
} {
  if (hasIdleCallbackSupport(target)) {
    return {
      schedule: (callback) =>
        target.requestIdleCallback(() => callback(), { timeout: 300 }),
      cancel: (handle) => target.cancelIdleCallback(handle),
    }
  }

  return {
    schedule: (callback) => target.setTimeout(callback, 120),
    cancel: (handle) => target.clearTimeout(handle),
  }
}
