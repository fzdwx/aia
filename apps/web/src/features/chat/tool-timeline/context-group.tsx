import { AnimatePresence, motion } from "motion/react"

import {
  AnimatedCountList,
  type CountItem,
} from "@/components/ai-elements/animated-count-list"
import { ToolStatusTitle } from "@/components/ai-elements/tool-status-title"
import { getFileDisplayParts, getToolDisplayPath } from "@/lib/tool-display"

import { toolRendererRegistry } from "@/features/chat/tool-rendering"
import {
  contextToolSummary,
  contextToolTrigger,
  type ToolRowItem,
} from "@/features/chat/tool-timeline-helpers"
import { toolTimelineCopy } from "@/features/chat/tool-timeline-copy"

const CONTEXT_GROUP_TRANSITION = {
  height: { duration: 0.18, ease: [0.16, 1, 0.3, 1] },
} as const

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
    trigger.title === "Read" ||
    trigger.title === "Write" ||
    trigger.title === "Edit"
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

export function ContextToolGroupList({ items }: { items: ToolRowItem[] }) {
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

export function ContextToolGroup({
  items,
  isRunning,
  open,
  onToggle,
}: {
  items: ToolRowItem[]
  isRunning: boolean
  open: boolean
  onToggle: () => void
}) {
  const countItems = buildCountItems(items)

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
          onClick={onToggle}
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
        {open ? <ContextToolGroupList items={items} /> : null}
      </AnimatePresence>
    </div>
  )
}
