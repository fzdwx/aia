import { memo, useEffect, useRef, useState } from "react"
import { AnimatePresence, motion } from "motion/react"

import { TextShimmer } from "@/components/ai-elements/text-shimmer"
import { ToolStatusTitle } from "@/components/ai-elements/tool-status-title"
import {
  AnimatedCountList,
  type CountItem,
} from "@/components/ai-elements/animated-count-list"
import { getToolDisplayName } from "@/lib/tool-display"
import { cn } from "@/lib/utils"
import type { StreamingToolOutput } from "@/lib/types"

import { toolRendererRegistry } from "./tool-rendering"
import { getFileDisplayParts, getToolDisplayPath } from "@/lib/tool-display"
import { buildDetailEntries } from "./tool-rendering/helpers"
import {
  DetailList,
  ToolDetailSurface,
  ToolInfoSection,
} from "./tool-rendering/ui"
import {
  coalesceStreamingToolOutputs,
  contextToolSummary,
  contextToolTrigger,
  formatDurationMs,
  fromStreamingTool,
  isContextExplorationTool,
  normalizeToolName,
  type ToolRowItem,
} from "./tool-timeline-helpers"
import { toolTimelineCopy } from "./tool-timeline-copy"

const ACTIVE_DURATION_TICK_MS = 100
const TOOL_DETAILS_TRANSITION = {
  height: { duration: 0.18, ease: [0.16, 1, 0.3, 1] },
  opacity: { duration: 0.12, ease: "linear" },
} as const
const CONTEXT_GROUP_TRANSITION = {
  height: { duration: 0.18, ease: [0.16, 1, 0.3, 1] },
} as const
const INLINE_DETAIL_TOOLS = new Set<string>()
const FLAT_DETAIL_SURFACE_TOOLS = new Set(["Edit", "Write", "ApplyPatch"])
const NON_DEFAULT_TOOL_NAMES = new Set([
  "Read",
  "Write",
  "Edit",
  "CodeSearch",
  "WebSearch",
  "Glob",
  "Grep",
  "Shell",
  "ApplyPatch",
  "question",
  "TapeInfo",
  "TapeHandoff",
])
const OMITTED_ARGUMENT_KEYS = new Set([
  "content",
  "patch",
  "patchText",
  "old_string",
  "new_string",
  "value",
  "text",
  "input",
  "contents",
])
const OMITTED_DETAIL_KEYS = new Set([
  "stdout",
  "stderr",
  "diff",
  "content",
  "file_path",
  "path",
  "pattern",
  "command",
])
const EMPTY_TOOL_RESULT_FALLBACK = "No output returned."
const FAILED_TOOL_RESULT_FALLBACK = "Tool execution failed."
const EMPTY_QUESTION_RESULT_FALLBACK = "No additional details."
const IGNORED_QUESTION_RESULT_FALLBACK = "Question ignored."

function hasText(value: string | undefined | null): value is string {
  return typeof value === "string" && value.trim().length > 0
}

function hasMeaningfulValue(value: unknown): boolean {
  if (value == null) return false
  if (typeof value === "string") return value.trim().length > 0
  if (Array.isArray(value))
    return value.some((entry) => hasMeaningfulValue(entry))
  if (typeof value === "object") {
    return Object.values(value as Record<string, unknown>).some((entry) =>
      hasMeaningfulValue(entry)
    )
  }
  return true
}

function getStringRecordValue(
  record: Record<string, unknown> | undefined,
  ...keys: string[]
): string | null {
  if (!record) return null

  for (const key of keys) {
    const value = record[key]
    if (typeof value === "string" && value.trim().length > 0) {
      return value
    }
  }

  return null
}

function getBooleanRecordValue(
  record: Record<string, unknown> | undefined,
  ...keys: string[]
): boolean {
  if (!record) return false

  return keys.some((key) => record[key] === true)
}

function isIgnoredQuestion(item: ToolRowItem): boolean {
  if (normalizeToolName(item.toolName) !== "question") return false

  const status = getStringRecordValue(item.details, "status", "state")
  const action = getStringRecordValue(item.details, "action")

  return (
    getBooleanRecordValue(
      item.details,
      "ignored",
      "was_ignored",
      "skip_render"
    ) ||
    getBooleanRecordValue(item.arguments, "ignored") ||
    status?.toLowerCase() === "ignored" ||
    action?.toLowerCase() === "ignore" ||
    action?.toLowerCase() === "ignored" ||
    item.outputContent.toLowerCase().includes("ignored")
  )
}

function hasQuestionResolution(item: ToolRowItem): boolean {
  if (hasText(item.outputContent)) return true

  return hasMeaningfulValue({
    summary: getStringRecordValue(item.details, "summary"),
    answer: getStringRecordValue(item.details, "answer"),
    reason: getStringRecordValue(item.details, "reason"),
    message: getStringRecordValue(item.details, "message"),
  })
}

function shouldRenderToolItem(item: ToolRowItem): boolean {
  if (normalizeToolName(item.toolName) !== "question") return true
  if (!item.succeeded) return true
  if (isIgnoredQuestion(item)) return true
  return hasQuestionResolution(item)
}

function hasVisibleToolDetails(item: ToolRowItem): boolean {
  return hasText(item.outputContent) || hasMeaningfulValue(item.details)
}

function getFallbackSubtitle(item: ToolRowItem): string | null {
  if (normalizeToolName(item.toolName) === "question") {
    if (isIgnoredQuestion(item)) return IGNORED_QUESTION_RESULT_FALLBACK
    if (!item.succeeded) {
      return hasText(item.outputContent)
        ? item.outputContent.trim()
        : FAILED_TOOL_RESULT_FALLBACK
    }
    if (item.finishedAtMs != null && !hasQuestionResolution(item)) {
      return EMPTY_QUESTION_RESULT_FALLBACK
    }
    return null
  }

  if (!item.succeeded) {
    return hasText(item.outputContent)
      ? item.outputContent.trim()
      : FAILED_TOOL_RESULT_FALLBACK
  }

  if (item.finishedAtMs != null && !hasVisibleToolDetails(item)) {
    return EMPTY_TOOL_RESULT_FALLBACK
  }

  return null
}

function useDurationTicker(enabled: boolean) {
  const [, setTick] = useState(0)

  useEffect(() => {
    if (!enabled) return

    const timer = window.setInterval(() => {
      setTick((current) => current + 1)
    }, ACTIVE_DURATION_TICK_MS)

    return () => window.clearInterval(timer)
  }, [enabled])
}

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
    succeeded: item.succeeded,
  }
  const displayName = getToolDisplayName(item.toolName)
  const title = toolRendererRegistry.renderTitle(renderData)
  const meta = isRunning ? null : toolRendererRegistry.renderMeta(renderData)
  const renderedSubtitle = toolRendererRegistry.renderSubtitle(renderData)
  const subtitle =
    getFallbackSubtitle(item) ??
    renderedSubtitle ??
    (title && title !== displayName ? title : null)
  const normalizedToolName = normalizeToolName(item.toolName)
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
      {!isRunning && fileParts ? (
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
      ) : !isRunning && subtitle ? (
        <span
          data-slot="tool-subtitle"
          data-state={item.succeeded ? "default" : "failure"}
          title={title ?? undefined}
        >
          {subtitle}
        </span>
      ) : null}
      {!isRunning && meta ? <span data-slot="tool-meta">{meta}</span> : null}
      {!isRunning && duration ? (
        <span data-slot="tool-duration">{duration}</span>
      ) : null}
    </div>
  )
}

function shouldInlineToolDetails(item: ToolRowItem) {
  return INLINE_DETAIL_TOOLS.has(normalizeToolName(item.toolName))
}

function shouldShowToolRowCaret() {
  return false
}

function renderToolDetailsPanel(item: ToolRowItem) {
  const normalizedToolName = normalizeToolName(item.toolName)

  if (normalizedToolName === "TapeInfo") {
    return null
  }

  const requestOmitKeys =
    normalizedToolName === "Shell"
      ? new Set([...OMITTED_ARGUMENT_KEYS, "command", "description"])
      : OMITTED_ARGUMENT_KEYS
  const renderData = {
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    succeeded: item.succeeded,
  }
  const detailsContent = toolRendererRegistry.renderDetails(renderData)
  const requestEntries = buildDetailEntries(item.arguments, {
    omitKeys: requestOmitKeys,
  })
  const resultEntries = buildDetailEntries(item.details, {
    omitKeys: OMITTED_DETAIL_KEYS,
  })
  const usesDefaultRenderer = !NON_DEFAULT_TOOL_NAMES.has(normalizedToolName)

  if (usesDefaultRenderer && detailsContent != null) {
    return <ToolDetailSurface>{detailsContent}</ToolDetailSurface>
  }

  if (normalizedToolName === "Shell") {
    if (detailsContent == null) return null

    return (
      <ToolDetailSurface className="tool-timeline-detail-surface-flat">
        {detailsContent}
      </ToolDetailSurface>
    )
  }

  if (
    FLAT_DETAIL_SURFACE_TOOLS.has(normalizeToolName(item.toolName)) &&
    detailsContent != null
  ) {
    return (
      <ToolDetailSurface className="tool-timeline-detail-surface-flat">
        {detailsContent}
      </ToolDetailSurface>
    )
  }

  if (
    requestEntries.length === 0 &&
    resultEntries.length === 0 &&
    detailsContent == null
  ) {
    return null
  }

  return (
    <ToolDetailSurface>
      {requestEntries.length > 0 ? (
        <ToolInfoSection
          title={toolTimelineCopy.section.request}
          hint={`${requestEntries.length} ${toolTimelineCopy.unit.field}`}
        >
          <DetailList entries={requestEntries} />
        </ToolInfoSection>
      ) : null}
      {resultEntries.length > 0 ? (
        <ToolInfoSection
          title={toolTimelineCopy.section.result}
          hint={`${resultEntries.length} ${toolTimelineCopy.unit.field}`}
          defaultOpen={false}
        >
          <DetailList entries={resultEntries} />
        </ToolInfoSection>
      ) : null}
      {detailsContent ? detailsContent : null}
    </ToolDetailSurface>
  )
}

function ToolRow({ item }: { item: ToolRowItem }) {
  const [showDetails, setShowDetails] = useState(false)
  useDurationTicker(item.finishedAtMs == null)
  const isRunning = item.finishedAtMs == null
  const duration = formatDurationMs(item.startedAtMs, item.finishedAtMs, {
    live: isRunning,
  })
  const renderData = {
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    succeeded: item.succeeded,
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
          setShowDetails(!showDetails)
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

function ContextToolTriggerRow({ item }: { item: ToolRowItem }) {
  const trigger = contextToolTrigger(item)
  const renderedTitle = toolRendererRegistry.renderTitle({
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    succeeded: item.succeeded,
  })
  const renderedSubtitle = toolRendererRegistry.renderSubtitle({
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    succeeded: item.succeeded,
  })
  const subtitle =
    renderedSubtitle ??
    (renderedTitle && renderedTitle !== trigger.title
      ? renderedTitle
      : trigger.subtitle)
  const isFileTool =
    trigger.title === "Read" || trigger.title === "Write" || trigger.title === "Edit"
  const filePath = isFileTool
    ? getToolDisplayPath(item.toolName, item.details, item.arguments)
    : ""
  const fileParts = filePath ? getFileDisplayParts(filePath) : null

  return (
    <div data-component="context-tool-trigger-row">
      <span data-slot="tool-title">{trigger.title}</span>
      {fileParts ? (
        <span data-slot="tool-subtitle" data-kind="file-path">
          <span data-slot="tool-file-name">{fileParts.fileName}</span>
          {fileParts.directory ? (
            <span data-slot="tool-file-dir">{fileParts.directory}</span>
          ) : null}
        </span>
      ) : subtitle ? (
        <span data-slot="tool-subtitle">{subtitle}</span>
      ) : null}
      {trigger.meta.length > 0 ? (
        <span data-slot="tool-meta">
          {trigger.meta.map((entry) => (
            <span key={entry}>{entry}</span>
          ))}
        </span>
      ) : null}
      {trigger.args.map((arg) => (
        <span key={arg.key} data-slot="tool-arg">
          {arg.key}={arg.value}
        </span>
      ))}
    </div>
  )
}

function ContextToolGroupList({ items }: { items: ToolRowItem[] }) {
  return (
    <motion.div
      key="list"
      data-component="context-tool-group-list"
      initial={{ height: 0 }}
      animate={{ height: "auto" }}
      exit={{ height: 0 }}
      transition={CONTEXT_GROUP_TRANSITION}
      style={{ overflow: "hidden" }}
    >
      <div data-slot="context-tool-group-list-inner">
        {items.map((item) => (
          <ContextToolTriggerRow key={item.id} item={item} />
        ))}
      </div>
    </motion.div>
  )
}

function buildCountItems(items: ToolRowItem[]): CountItem[] {
  const summary = contextToolSummary(items)
  const countItems: CountItem[] = []
  const { contextCount } = toolTimelineCopy

  if (summary.read > 0) {
    countItems.push({
      key: "read",
      count: summary.read,
      ...contextCount.read,
    })
  }
  if (summary.search > 0) {
    countItems.push({
      key: "search",
      count: summary.search,
      ...contextCount.search,
    })
  }
  if (summary.list > 0) {
    countItems.push({
      key: "list",
      count: summary.list,
      ...contextCount.list,
    })
  }

  return countItems
}

export function ToolGroup({
  items,
  status = "completed",
  keepContextGroupsOpen = false,
}: {
  items: ToolRowItem[]
  status?: "running" | "completed"
  keepContextGroupsOpen?: boolean
}) {
  const visibleItems = items.filter(shouldRenderToolItem)
  const isContextGroup = visibleItems.every((item) =>
    isContextExplorationTool(item.toolName)
  )
  const isRunning = status === "running"
  const shouldKeepOpen = isContextGroup && (isRunning || keepContextGroupsOpen)
  const [open, setOpen] = useState(shouldKeepOpen)
  const wasOpenRef = useRef(shouldKeepOpen)
  const countItems = isContextGroup ? buildCountItems(visibleItems) : []

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
          <ToolRow key={item.id} item={item} />
        ))}
      </div>
    )
  }

  return (
    <div data-component="tool-group" data-variant="context">
      {isRunning ? (
        <div data-component="context-tool-group-trigger">
          <span className="size-1.5 shrink-0 animate-pulse rounded-full bg-amber-500/70" />
          <ToolStatusTitle
            active
            activeText={toolTimelineCopy.groupStatus.running}
            doneText={toolTimelineCopy.groupStatus.completed}
          />
          <AnimatedCountList items={countItems} />
        </div>
      ) : (
        <button
          type="button"
          onClick={() => setOpen((current) => !current)}
          aria-expanded={open}
          data-interactive="true"
          data-component="context-tool-group-trigger"
          className="transition-colors hover:text-foreground focus-visible:outline-none"
        >
          <ToolStatusTitle
            active={false}
            activeText={toolTimelineCopy.groupStatus.running}
            doneText={toolTimelineCopy.groupStatus.completed}
          />
          <span
            data-slot="context-group-counts-shell"
            data-state={open ? "hidden" : "visible"}
            aria-hidden={open}
          >
            <AnimatedCountList items={countItems} />
          </span>
        </button>
      )}
      <AnimatePresence initial={false}>
        {open && <ContextToolGroupList items={visibleItems} />}
      </AnimatePresence>
    </div>
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
  useDurationTicker(active.length > 0)

  if (coalescedToolOutputs.length === 0) return null

  return (
    <div data-component="tool-timeline-stream">
      {completed.length > 0 && (
        <ToolGroup
          key="completed"
          items={completed.map(fromStreamingTool)}
          keepContextGroupsOpen={keepContextGroupsOpen}
        />
      )}

      {active.length > 0 && (
        <ToolGroup
          key="active"
          items={active.map(fromStreamingTool)}
          status="running"
          keepContextGroupsOpen={keepContextGroupsOpen}
        />
      )}
    </div>
  )
}

export const MemoizedToolGroup = memo(ToolGroup)
export const MemoizedStreamingToolGroup = memo(StreamingToolGroup)
