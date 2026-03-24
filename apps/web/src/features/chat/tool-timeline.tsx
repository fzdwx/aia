import { ChevronDown, ChevronRight } from "lucide-react"
import { memo, useEffect, useState } from "react"

import { Shimmer } from "@/components/ai-elements/shimmer"
import { getToolDisplayName } from "@/lib/tool-display"
import { cn } from "@/lib/utils"
import type { StreamingToolOutput } from "@/lib/types"

import { toolRendererRegistry } from "./tool-rendering"
import { buildDetailEntries } from "./tool-rendering/helpers"
import {
  DetailList,
  ToolDetailSurface,
  ToolInfoSection,
} from "./tool-rendering/ui"
import {
  buildCategorySummary,
  formatDurationMs,
  fromStreamingTool,
  type ToolRowItem,
} from "./tool-timeline-helpers"

const ACTIVE_DURATION_TICK_MS = 100
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

function ToolSummaryLine({
  item,
  duration,
}: {
  item: ToolRowItem
  duration: string | null
}) {
  const isRunning = item.finishedAtMs == null
  const title = toolRendererRegistry.renderTitle({
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    succeeded: item.succeeded,
  })
  const meta = toolRendererRegistry.renderMeta({
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    succeeded: item.succeeded,
  })

  return (
    <div className="flex min-w-0 items-baseline justify-between gap-3">
      <div className="flex min-w-0 flex-1 items-baseline gap-x-2 gap-y-1">
        <span className="text-body-sm shrink-0 font-semibold text-foreground/94">
          {getToolDisplayName(item.toolName)}
        </span>
        <p
          title={title || getToolDisplayName(item.toolName)}
          className={cn(
            "text-ui min-w-0 truncate text-left",
            item.succeeded || isRunning
              ? "text-muted-foreground/82"
              : "text-destructive"
          )}
        >
          {title || getToolDisplayName(item.toolName)}
        </p>
      </div>
      {(meta || duration) && (
        <div className="flex shrink-0 items-baseline gap-x-2 gap-y-1 pl-3">
          {meta ? (
            <div className="text-ui flex shrink-0 flex-wrap items-baseline gap-x-2 gap-y-1 text-muted-foreground/68">
              {meta}
            </div>
          ) : null}
          {duration ? (
            <span className="text-meta shrink-0 text-muted-foreground/56 tabular-nums">
              {duration}
            </span>
          ) : null}
        </div>
      )}
    </div>
  )
}

function renderToolDetailsPanel(item: ToolRowItem) {
  const renderData = {
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    succeeded: item.succeeded,
  }
  const detailsContent = toolRendererRegistry.renderDetails(renderData)
  const requestEntries = buildDetailEntries(item.arguments, {
    omitKeys: OMITTED_ARGUMENT_KEYS,
  })
  const resultEntries = buildDetailEntries(item.details, {
    omitKeys: OMITTED_DETAIL_KEYS,
  })

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
          title="Inputs"
          hint={`${requestEntries.length} field${requestEntries.length === 1 ? "" : "s"}`}
        >
          <DetailList entries={requestEntries} />
        </ToolInfoSection>
      ) : null}
      {resultEntries.length > 0 ? (
        <ToolInfoSection
          title="Result"
          hint={`${resultEntries.length} field${resultEntries.length === 1 ? "" : "s"}`}
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
  const detailsContent = renderToolDetailsPanel(item)
  const hasDetails = detailsContent != null
  const detailsId = `tool-details-${item.id}`

  return (
    <div className="w-full">
      <button
        onClick={() => {
          if (!hasDetails) return
          setShowDetails(!showDetails)
        }}
        aria-expanded={hasDetails ? showDetails : undefined}
        aria-controls={hasDetails ? detailsId : undefined}
        aria-disabled={!hasDetails}
        className={cn(
          "w-full px-0 py-1.5 text-left transition-colors focus-visible:outline-none",
          hasDetails ? "hover:text-foreground" : "cursor-default"
        )}
      >
        <div className="flex items-start gap-2">
          <div className="min-w-0 flex-1">
            <ToolSummaryLine item={item} duration={duration} />
          </div>
          {hasDetails ? (
            <span className="mt-0.5 text-muted-foreground/35">
              {showDetails ? (
                <ChevronDown className="size-3.5" />
              ) : (
                <ChevronRight className="size-3.5" />
              )}
            </span>
          ) : null}
        </div>
      </button>
      {showDetails && detailsContent && (
        <div id={detailsId} className="pt-1.5 pl-4">
          <div>{detailsContent}</div>
        </div>
      )}
    </div>
  )
}

export function ToolGroup({
  items,
  isStreaming = false,
}: {
  items: ToolRowItem[]
  isStreaming?: boolean
}) {
  const [open, setOpen] = useState(isStreaming)
  const summary = buildCategorySummary(items)

  useEffect(() => {
    if (isStreaming) {
      setOpen(true)
    }
  }, [isStreaming])

  return (
    <div className="mb-5 w-full">
      <button
        onClick={() => setOpen(!open)}
        className="text-ui flex items-center gap-2 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none"
      >
        <span className="font-medium">
          {isStreaming ? "Running" : "Explored"}
        </span>
        {!open && (
          <span className="text-meta mt-[2px] text-muted-foreground/70">
            {summary
              .map((s) => `${s.count} ${s.label}${s.count > 1 ? "s" : ""}`)
              .join(", ")}
          </span>
        )}
      </button>
      {open && (
        <div className="mt-2.5 space-y-3.5">
          {items.map((item) => (
            <ToolRow key={item.id} item={item} />
          ))}
        </div>
      )}
    </div>
  )
}

export function StreamingToolGroup({
  toolOutputs,
}: {
  toolOutputs: StreamingToolOutput[]
}) {
  const completed = toolOutputs.filter((t) => t.completed)
  const active = toolOutputs.filter((t) => !t.completed)
  const activeSummary = buildCategorySummary(active)
  useDurationTicker(active.length > 0)

  if (toolOutputs.length === 0) return null

  return (
    <div className="mb-3 w-full space-y-3">
      {completed.length > 0 && (
        <ToolGroup items={completed.map(fromStreamingTool)} isStreaming />
      )}

      {active.length > 0 && (
        <>
          <div className="text-ui flex items-center gap-2 text-muted-foreground">
            <span className="size-1.5 shrink-0 animate-pulse rounded-full bg-amber-500/70" />
            <Shimmer as="span" className="font-medium" duration={2}>
              Running tools
            </Shimmer>
            <span className="text-meta text-muted-foreground/70">
              {activeSummary
                .map((s) => `${s.count} ${s.label}${s.count > 1 ? "s" : ""}`)
                .join(", ")}
            </span>
          </div>
          <div className="space-y-3.5">
            {active.map((tool) => (
              <ToolRow key={tool.invocationId} item={fromStreamingTool(tool)} />
            ))}
          </div>
        </>
      )}
    </div>
  )
}

export const MemoizedToolGroup = memo(ToolGroup)
export const MemoizedStreamingToolGroup = memo(StreamingToolGroup)
