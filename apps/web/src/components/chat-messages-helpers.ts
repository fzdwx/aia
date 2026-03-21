export const HISTORY_LOAD_TRIGGER_PX = 80
export const HISTORY_HINT_VISIBILITY_PX = 160
export const STICK_TO_BOTTOM_THRESHOLD_PX = 120

export function clampScrollTop(target: number, maxScrollTop: number) {
  return Math.min(Math.max(target, 0), Math.max(maxScrollTop, 0))
}

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
