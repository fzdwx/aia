import { memo, useEffect, useRef, useState } from "react"
import { AnimatePresence, motion } from "motion/react"

import { TextShimmer } from "@/components/ai-elements/text-shimmer"
import { getToolDisplayName } from "@/lib/tool-display"
import { cn } from "@/lib/utils"
import type { StreamingToolOutput } from "@/lib/types"

import { toolRendererRegistry } from "./tool-rendering"
import { getFileDisplayParts, getToolDisplayPath } from "@/lib/tool-display"
import {
  coalesceStreamingToolOutputs,
  formatDurationMs,
  fromStreamingTool,
  isContextExplorationTool,
  normalizeToolName,
  type ToolRowItem,
} from "./tool-timeline-helpers"
import { ContextToolGroup } from "./tool-timeline/context-group"
import {
  getFallbackSubtitle,
  shouldInlineToolDetails,
  shouldRenderToolItem,
  shouldShowToolRowCaret,
} from "./tool-timeline/tool-row-policy"
import { renderToolDetailsPanel } from "./tool-timeline/tool-details-panel"
import { useDurationTicker } from "./tool-timeline/use-duration-ticker"

const TOOL_DETAILS_TRANSITION = {
  height: { duration: 0.18, ease: [0.16, 1, 0.3, 1] },
  opacity: { duration: 0.12, ease: "linear" },
} as const
function ToolTrigger({
  item,
  duration,
}: {
  item: ToolRowItem
  duration: string | null
}) {
  const isRunning = item.finishedAtMs == null
  const renderData = {
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    outputSegments: item.outputSegments,
    succeeded: item.succeeded,
    isRunning,
  }
  const displayName = getToolDisplayName(item.toolName)
  const title = toolRendererRegistry.renderTitle(renderData)
  const meta = toolRendererRegistry.renderMeta(renderData)
  const renderedSubtitle = toolRendererRegistry.renderSubtitle(renderData)
  const normalizedToolName = normalizeToolName(item.toolName)
  const prefersRenderedSubtitle = normalizedToolName === "Shell"
  const subtitle =
    (prefersRenderedSubtitle ? renderedSubtitle : getFallbackSubtitle(item)) ??
    (prefersRenderedSubtitle ? getFallbackSubtitle(item) : renderedSubtitle) ??
    (title && title !== displayName ? title : null)
  const isFileTool =
    normalizedToolName === "Read" ||
    normalizedToolName === "Write" ||
    normalizedToolName === "Edit"
  const filePath = isFileTool
    ? getToolDisplayPath(item.toolName, item.details, item.arguments)
    : ""
  const fileParts = filePath ? getFileDisplayParts(filePath) : null

  return (
    <div data-component="tool-trigger">
      <span data-slot="tool-title">
        {isRunning ? <TextShimmer text={displayName} active /> : displayName}
      </span>
      {fileParts ? (
        <span
          data-slot="tool-subtitle"
          data-kind="file-path"
          data-state={item.succeeded ? "default" : "failure"}
          title={title ?? undefined}
        >
          <span data-slot="tool-file-name">{fileParts.fileName}</span>
          {fileParts.directory ? (
            <span data-slot="tool-file-dir">{fileParts.directory}</span>
          ) : null}
        </span>
      ) : subtitle ? (
        <span
          data-slot="tool-subtitle"
          data-state={item.succeeded ? "default" : "failure"}
          title={title ?? undefined}
        >
          {subtitle}
        </span>
      ) : null}
      {meta ? <span data-slot="tool-meta">{meta}</span> : null}
      {duration ? <span data-slot="tool-duration">{duration}</span> : null}
    </div>
  )
}

function ToolRow({
  item,
  expanded,
  onExpandedChange,
}: {
  item: ToolRowItem
  expanded?: boolean
  onExpandedChange?: (id: string, expanded: boolean) => void
}) {
  const [localShowDetails, setLocalShowDetails] = useState(false)
  const showDetails = expanded ?? localShowDetails
  useDurationTicker(item.finishedAtMs == null)
  const isRunning = item.finishedAtMs == null
  const normalizedToolName = normalizeToolName(item.toolName)
  const durationStartMs = isRunning
    ? normalizedToolName === "Shell"
      ? item.startedAtMs
      : (item.startedAtMs ?? item.detectedAtMs)
    : (item.startedAtMs ?? item.detectedAtMs)
  const duration = formatDurationMs(durationStartMs, item.finishedAtMs, {
    live: isRunning,
  })
  const renderData = {
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    outputSegments: item.outputSegments,
    succeeded: item.succeeded,
    isRunning,
  }
  const inlineDetails = shouldInlineToolDetails(item)
    ? toolRendererRegistry.renderDetails(renderData)
    : null
  const detailsContent = inlineDetails ? null : renderToolDetailsPanel(item)
  const hasDetails = detailsContent != null
  const detailsId = `tool-details-${item.id}`

  return (
    <div data-component="tool-row">
      <button
        type="button"
        onClick={() => {
          if (!hasDetails) return
          if (onExpandedChange) {
            onExpandedChange(item.id, !showDetails)
            return
          }
          setLocalShowDetails(!localShowDetails)
        }}
        aria-expanded={hasDetails ? showDetails : undefined}
        aria-controls={hasDetails ? detailsId : undefined}
        aria-disabled={!hasDetails}
        data-expandable={hasDetails}
        data-show-caret={shouldShowToolRowCaret() || undefined}
        data-component="tool-row-trigger"
        className={cn(
          "focus-visible:outline-none",
          hasDetails ? "hover:text-foreground" : "cursor-default"
        )}
      >
        <div data-slot="tool-row-shell">
          <div data-slot="tool-row-main">
            <ToolTrigger item={item} duration={duration} />
          </div>
        </div>
      </button>
      {inlineDetails ? (
        <div data-slot="tool-row-inline-details">{inlineDetails}</div>
      ) : null}
      <AnimatePresence initial={false}>
        {showDetails && detailsContent && (
          <motion.div
            key="details"
            id={detailsId}
            data-slot="tool-row-details"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={TOOL_DETAILS_TRANSITION}
            style={{ overflow: "hidden" }}
          >
            <div>{detailsContent}</div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

export function ToolGroup({
  items,
  status = "completed",
  keepContextGroupsOpen = false,
  expandedToolIds,
  onExpandedChange,
}: {
  items: ToolRowItem[]
  status?: "running" | "completed"
  keepContextGroupsOpen?: boolean
  expandedToolIds?: ReadonlySet<string>
  onExpandedChange?: (id: string, expanded: boolean) => void
}) {
  const visibleItems = items.filter(shouldRenderToolItem)
  const isContextGroup = visibleItems.every((item) =>
    isContextExplorationTool(item.toolName)
  )
  const isRunning = status === "running"
  const shouldKeepOpen = isContextGroup && (isRunning || keepContextGroupsOpen)
  const [open, setOpen] = useState(shouldKeepOpen)
  const wasOpenRef = useRef(shouldKeepOpen)

  useEffect(() => {
    if (!isContextGroup) {
      wasOpenRef.current = shouldKeepOpen
      return
    }

    if (shouldKeepOpen) {
      setOpen(true)
    } else if (wasOpenRef.current) {
      setOpen(false)
    }

    wasOpenRef.current = shouldKeepOpen
  }, [isContextGroup, shouldKeepOpen])

  if (visibleItems.length === 0) return null

  if (!isContextGroup) {
    return (
      <div data-component="tool-group" data-variant="standalone">
        {visibleItems.map((item) => (
          <ToolRow
            key={item.id}
            item={item}
            expanded={expandedToolIds?.has(item.id)}
            onExpandedChange={onExpandedChange}
          />
        ))}
      </div>
    )
  }

  return (
    <ContextToolGroup
      items={visibleItems}
      isRunning={isRunning}
      open={open}
      onToggle={() => setOpen((current) => !current)}
    />
  )
}

export function StreamingToolGroup({
  toolOutputs,
  keepContextGroupsOpen = false,
}: {
  toolOutputs: StreamingToolOutput[]
  keepContextGroupsOpen?: boolean
}) {
  const coalescedToolOutputs = coalesceStreamingToolOutputs(toolOutputs)
  const completed = coalescedToolOutputs.filter((t) => t.completed)
  const active = coalescedToolOutputs.filter((t) => !t.completed)
  const [expandedToolIds, setExpandedToolIds] = useState<Set<string>>(new Set())
  useDurationTicker(active.length > 0)

  useEffect(() => {
    const visibleIds = new Set(
      coalescedToolOutputs.map((tool) => tool.invocationId)
    )
    setExpandedToolIds((current) => {
      const next = new Set(
        [...current].filter((invocationId) => visibleIds.has(invocationId))
      )
      return next.size === current.size ? current : next
    })
  }, [coalescedToolOutputs])

  const handleExpandedChange = (id: string, expanded: boolean) => {
    setExpandedToolIds((current) => {
      const next = new Set(current)
      if (expanded) {
        next.add(id)
      } else {
        next.delete(id)
      }
      return next
    })
  }

  if (coalescedToolOutputs.length === 0) return null

  return (
    <div data-component="tool-timeline-stream">
      {completed.length > 0 && (
        <ToolGroup
          key="completed"
          items={completed.map(fromStreamingTool)}
          keepContextGroupsOpen={keepContextGroupsOpen}
          expandedToolIds={expandedToolIds}
          onExpandedChange={handleExpandedChange}
        />
      )}

      {active.length > 0 && (
        <ToolGroup
          key="active"
          items={active.map(fromStreamingTool)}
          status="running"
          keepContextGroupsOpen={keepContextGroupsOpen}
          expandedToolIds={expandedToolIds}
          onExpandedChange={handleExpandedChange}
        />
      )}
    </div>
  )
}

export const MemoizedToolGroup = memo(ToolGroup)
export const MemoizedStreamingToolGroup = memo(StreamingToolGroup)
