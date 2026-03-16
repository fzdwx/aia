export type MeasuredWindow = {
  startIndex: number
  endIndex: number
  topSpacerHeight: number
  bottomSpacerHeight: number
}

type BuildMeasuredWindowOptions = {
  itemIds: string[]
  containerHeight: number
  scrollTop: number
  measuredHeights: Record<string, number>
  overscan?: number
  minItems?: number
  defaultItemHeight?: number
}

type CalculateAnchorScrollTopOptions = {
  currentScrollTop: number
  currentOffset: number
  desiredOffset: number
  maxScrollTop: number
}

export function buildMeasuredWindow({
  itemIds,
  containerHeight,
  scrollTop,
  measuredHeights,
  overscan = 4,
  minItems = 40,
  defaultItemHeight = 280,
}: BuildMeasuredWindowOptions): MeasuredWindow {
  if (itemIds.length < minItems || containerHeight <= 0) {
    return {
      startIndex: 0,
      endIndex: itemIds.length,
      topSpacerHeight: 0,
      bottomSpacerHeight: 0,
    }
  }

  const heights = itemIds.map(
    (itemId) => measuredHeights[itemId] ?? defaultItemHeight
  )

  let accumulatedHeight = 0
  let startIndex = 0
  while (
    startIndex < heights.length &&
    accumulatedHeight + heights[startIndex]! < scrollTop
  ) {
    accumulatedHeight += heights[startIndex]!
    startIndex += 1
  }

  let endIndex = startIndex
  let visibleHeight = 0
  while (endIndex < heights.length && visibleHeight < containerHeight) {
    visibleHeight += heights[endIndex]!
    endIndex += 1
  }

  startIndex = Math.max(0, startIndex - overscan)
  endIndex = Math.min(itemIds.length, endIndex + overscan)

  return {
    startIndex,
    endIndex,
    topSpacerHeight: heights
      .slice(0, startIndex)
      .reduce((sum, height) => sum + height, 0),
    bottomSpacerHeight: heights
      .slice(endIndex)
      .reduce((sum, height) => sum + height, 0),
  }
}

export function calculateAnchorScrollTop({
  currentScrollTop,
  currentOffset,
  desiredOffset,
  maxScrollTop,
}: CalculateAnchorScrollTopOptions): number {
  const unclamped = currentScrollTop + currentOffset - desiredOffset
  if (unclamped < 0) return 0
  if (unclamped > maxScrollTop) return maxScrollTop
  return unclamped
}
