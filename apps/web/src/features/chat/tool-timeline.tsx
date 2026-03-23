import { Check, ChevronDown, ChevronRight, Clock3 } from "lucide-react"
import { memo, useEffect, useState } from "react"

import { Shimmer } from "@/components/ai-elements/shimmer"
import { Badge } from "@/components/ui/badge"
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
    <div className="grid min-w-0 grid-cols-[minmax(58px,max-content)_minmax(0,1fr)_auto] items-center gap-x-2 gap-y-1 text-[12px]">
      <div className="flex min-w-0 items-center gap-1.5">
        <Badge
          variant="outline"
          className="border-border/50 bg-background/75 text-[10px] text-foreground/85"
        >
          {getToolDisplayName(item.toolName)}
        </Badge>
      </div>
      <p
        title={title || getToolDisplayName(item.toolName)}
        className={cn(
          "truncate text-left leading-5",
          item.succeeded || isRunning
            ? "text-foreground/85"
            : "text-destructive"
        )}
      >
        {title || getToolDisplayName(item.toolName)}
      </p>
      <div className="flex shrink-0 items-center gap-1.5 pl-1 text-[11px] text-muted-foreground/70">
        {meta ? (
          <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground/75">
            {meta}
          </div>
        ) : null}
        {duration ? (
          <span className="inline-flex items-center gap-1 tabular-nums">
            <Clock3 className="size-3" />
            {duration}
          </span>
        ) : null}
      </div>
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
          title="Request"
          hint={`${requestEntries.length} field${requestEntries.length === 1 ? "" : "s"}`}
        >
          <DetailList entries={requestEntries} />
        </ToolInfoSection>
      ) : null}
      {resultEntries.length > 0 ? (
        <ToolInfoSection
          title="Result details"
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
    <div
      className={cn(
        "rounded-xl border transition-colors",
        isRunning
          ? "border-amber-500/20 bg-amber-500/[0.04]"
          : item.succeeded
            ? "border-border/30 bg-background/55 hover:border-border/45"
            : "border-destructive/20 bg-destructive/[0.03]"
      )}
    >
      <button
        onClick={() => {
          if (!hasDetails) return
          setShowDetails(!showDetails)
        }}
        aria-expanded={hasDetails ? showDetails : undefined}
        aria-controls={hasDetails ? detailsId : undefined}
        aria-disabled={!hasDetails}
        className="w-full px-3 py-2.5 text-left"
      >
        <div className="flex items-center gap-3">
          <span
            className={cn(
              "size-2 shrink-0 rounded-full",
              isRunning
                ? "bg-amber-500"
                : item.succeeded
                  ? "bg-foreground/50"
                  : "bg-destructive"
            )}
          />
          <div className="min-w-0 flex-1">
            <ToolSummaryLine item={item} duration={duration} />
          </div>
          {hasDetails ? (
            <span className="text-muted-foreground/60">
              {showDetails ? (
                <ChevronDown className="size-4" />
              ) : (
                <ChevronRight className="size-4" />
              )}
            </span>
          ) : null}
        </div>
      </button>
      {showDetails && detailsContent && (
        <div id={detailsId} className="border-t border-border/20 px-3 pb-3">
          <div className="pt-3">{detailsContent}</div>
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
  const allSucceeded = items.every((item) => item.succeeded)
  const summary = buildCategorySummary(items)

  useEffect(() => {
    if (isStreaming) {
      setOpen(true)
    }
  }, [isStreaming])

  return (
    <div className="mb-3">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-2 text-[13px] text-muted-foreground transition-colors hover:text-foreground"
      >
        <span className="font-medium">
          {isStreaming ? "Exploring" : "Explored"}
        </span>
        {!open && (
          <span className="text-muted-foreground/70">
            {summary
              .map((s) => `${s.count} ${s.label}${s.count > 1 ? "s" : ""}`)
              .join(", ")}
          </span>
        )}
        {allSucceeded && <Check className="size-3.5 text-foreground/55" />}
      </button>
      {open && (
        <div className="mt-2 ml-4 space-y-2">
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
    <div className="mb-2">
      {completed.length > 0 && (
        <ToolGroup items={completed.map(fromStreamingTool)} isStreaming />
      )}

      {active.length > 0 && (
        <>
          <div className="flex items-center gap-2 text-[13px] text-muted-foreground">
            <span className="size-1.5 shrink-0 animate-pulse rounded-full bg-amber-500/70" />
            <Shimmer as="span" className="font-medium" duration={2}>
              Running tools
            </Shimmer>
            <span className="text-muted-foreground/70">
              {activeSummary
                .map((s) => `${s.count} ${s.label}${s.count > 1 ? "s" : ""}`)
                .join(", ")}
            </span>
          </div>
          <div className="mt-2 ml-4 space-y-2">
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
