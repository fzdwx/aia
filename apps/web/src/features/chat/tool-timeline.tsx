import { Check, X as XIcon } from "lucide-react"
import { memo, useEffect, useState } from "react"

import { Shimmer } from "@/components/ai-elements/shimmer"
import { getToolDisplayName } from "@/lib/tool-display"
import type { StreamingToolOutput } from "@/lib/types"

import { toolRendererRegistry } from "./tool-rendering"
import {
  buildCategorySummary,
  formatDurationMs,
  fromStreamingTool,
  getToolStats,
  type ToolRowItem,
} from "./tool-timeline-helpers"

function ToolRow({ item }: { item: ToolRowItem }) {
  const [showDetails, setShowDetails] = useState(false)
  const stats = getToolStats(item.details)
  const title = toolRendererRegistry.renderTitle({
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    succeeded: item.succeeded,
  })
  const detailsContent = toolRendererRegistry.renderDetails({
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    succeeded: item.succeeded,
  })
  const duration = formatDurationMs(item.startedAtMs, item.finishedAtMs)

  return (
    <div>
      <button
        onClick={() => setShowDetails(!showDetails)}
        className="grid w-full grid-cols-[minmax(56px,max-content)_1fr_auto] items-center gap-x-2 py-0.5 text-[12px] text-muted-foreground transition-colors hover:text-foreground"
      >
        <span className="truncate text-left font-medium text-muted-foreground/70">
          {getToolDisplayName(item.toolName)}
        </span>
        <span className="truncate text-left">{title}</span>
        <div className="flex items-center gap-2">
          {stats.added != null && (
            <span className="shrink-0 text-emerald-500">+{stats.added}</span>
          )}
          {stats.removed != null && (
            <span className="shrink-0 text-red-400">-{stats.removed}</span>
          )}
          {stats.lines != null && (
            <span className="shrink-0 text-emerald-500">+{stats.lines}</span>
          )}
          {stats.matches != null && !stats.truncated && (
            <span className="shrink-0 text-muted-foreground/50">
              {stats.matches} matches
            </span>
          )}
          {stats.truncated && stats.matches != null && (
            <span className="shrink-0 text-amber-600/80">
              {stats.matches} matches (showing {stats.returned})
            </span>
          )}
          {stats.linesRead != null && stats.totalLines != null && (
            <span className="shrink-0 text-muted-foreground/50">
              {stats.linesRead}/{stats.totalLines}
            </span>
          )}
          {item.succeeded ? (
            <Check className="size-3 shrink-0 text-foreground/30" />
          ) : (
            <XIcon className="size-3 shrink-0 text-destructive/70" />
          )}
          {duration && (
            <span className="shrink-0 text-muted-foreground/50">
              {duration}
            </span>
          )}
        </div>
      </button>
      {showDetails && detailsContent && (
        <div className="mt-1 mb-2 ml-3 space-y-2.5 rounded-md border border-border/25 bg-muted/15 p-2">
          {detailsContent}
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
        className="flex items-center gap-1.5 text-[13px] text-muted-foreground transition-colors hover:text-foreground"
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
        {allSucceeded && <Check className="size-3.5 text-emerald-500/70" />}
      </button>
      {open && (
        <div className="mt-1 ml-5">
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
  if (toolOutputs.length === 0) return null

  const completed = toolOutputs.filter((t) => t.completed)
  const active = toolOutputs.filter((t) => !t.completed)
  const activeSummary = buildCategorySummary(active)

  return (
    <div className="mb-2">
      {completed.length > 0 && (
        <ToolGroup items={completed.map(fromStreamingTool)} isStreaming />
      )}

      {active.length > 0 && (
        <>
          <div className="flex items-center gap-1.5 text-[13px] text-muted-foreground">
            <span className="size-1.5 shrink-0 animate-pulse rounded-full bg-amber-500/70" />
            <Shimmer as="span" className="font-medium" duration={2}>
              Exploring
            </Shimmer>
            <span className="text-muted-foreground/70">
              {activeSummary
                .map((s) => `${s.count} ${s.label}${s.count > 1 ? "s" : ""}`)
                .join(", ")}
            </span>
          </div>
          <div className="mt-0.5 ml-3 space-y-0.5">
            {active.map((tool) => {
              const title = toolRendererRegistry.renderTitle({
                toolName: tool.toolName,
                arguments: tool.arguments,
                details: tool.resultDetails ?? undefined,
                outputContent: tool.resultContent ?? tool.output,
                succeeded: !tool.failed,
              })
              return (
                <div
                  key={tool.invocationId}
                  className="grid grid-cols-[minmax(48px,max-content)_1fr_auto] items-center gap-x-2 py-0.5 text-[13px] text-muted-foreground/60"
                >
                  {tool.toolName && (
                    <span className="truncate text-left font-medium">
                      {getToolDisplayName(tool.toolName)}
                    </span>
                  )}
                  <span className="truncate text-left">
                    {title || "preparing"}
                  </span>
                  <span className="shrink-0 text-muted-foreground/50">
                    {tool.startedAtMs
                      ? (formatDurationMs(
                          tool.startedAtMs,
                          tool.finishedAtMs
                        ) ?? "0 ms")
                      : "queued"}
                  </span>
                </div>
              )
            })}
          </div>
        </>
      )}
    </div>
  )
}

export const MemoizedToolGroup = memo(ToolGroup)
export const MemoizedStreamingToolGroup = memo(StreamingToolGroup)
