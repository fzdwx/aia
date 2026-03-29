export const HISTORY_LOAD_TRIGGER_PX = 400
export const HISTORY_HINT_VISIBILITY_PX = 160
export const STICK_TO_BOTTOM_THRESHOLD_PX = 24

export function distanceFromBottom({
  scrollHeight,
  scrollTop,
  clientHeight,
}: {
  scrollHeight: number
  scrollTop: number
  clientHeight: number
}) {
  return scrollHeight - scrollTop - clientHeight
}

export function shouldStickToBottom(distance: number) {
  return distance < STICK_TO_BOTTOM_THRESHOLD_PX
}

export function shouldShowHistoryHint(
  historyLoadingMore: boolean,
  scrollTop: number
) {
  return historyLoadingMore || scrollTop < HISTORY_HINT_VISIBILITY_PX
}

export function shouldTriggerOlderTurnsLoad(scrollTop: number) {
  return scrollTop <= HISTORY_LOAD_TRIGGER_PX
}

export function shouldLoadOlderTurnsOnScroll({
  scrollTop,
  scrollHeight,
  clientHeight,
  userScrolledUp,
}: {
  scrollTop: number
  scrollHeight: number
  clientHeight: number
  userScrolledUp: boolean
}) {
  if (!userScrolledUp) return false
  if (scrollHeight <= clientHeight) return false
  return shouldTriggerOlderTurnsLoad(scrollTop)
}
