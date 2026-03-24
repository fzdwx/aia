import { useState } from "react"
import type { ReactNode } from "react"

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { cn } from "@/lib/utils"

import { toolTimelineCopy } from "../tool-timeline-copy"

type ToolSectionKind =
  | "request"
  | "result"
  | "content"
  | "failure"
  | "issue"
  | "issue-ignored"
  | "patch"
  | "raw-details"
  | "top-result"
  | "other"

const TOOL_SECTION_ALIASES: Record<string, ToolSectionKind> = {
  [toolTimelineCopy.section.request.toLowerCase()]: "request",
  request: "request",
  input: "request",
  [toolTimelineCopy.section.result.toLowerCase()]: "result",
  result: "result",
  [toolTimelineCopy.section.content.toLowerCase()]: "content",
  content: "content",
  [toolTimelineCopy.section.failure.toLowerCase()]: "failure",
  failure: "failure",
  [toolTimelineCopy.section.issue.toLowerCase()]: "issue",
  issue: "issue",
  [toolTimelineCopy.section.issueIgnored.toLowerCase()]: "issue-ignored",
  "issue ignored": "issue-ignored",
  [toolTimelineCopy.section.patch.toLowerCase()]: "patch",
  patch: "patch",
  [toolTimelineCopy.section.rawDetails.toLowerCase()]: "raw-details",
  "raw details": "raw-details",
  "top result": "top-result",
}

function resolveToolSectionKind(title: string): ToolSectionKind {
  return TOOL_SECTION_ALIASES[title.trim().toLowerCase()] ?? "other"
}

function resolveToolSectionTone(kind: ToolSectionKind) {
  switch (kind) {
    case "result":
    case "content":
    case "top-result":
      return "output"
    case "failure":
    case "issue":
      return "failure"
    case "patch":
      return "patch"
    default:
      return "neutral"
  }
}

export function ExpandableOutput({
  value,
  failed,
}: {
  value: string
  failed: boolean
}) {
  const [open, setOpen] = useState(false)
  const needsCollapse = value.length > 280 || value.split("\n").length > 10
  const lineCount = value.split("\n").length

  return (
    <div className="tool-timeline-output">
      <pre
        data-failed={failed || undefined}
        className={cn(
          "tool-timeline-output-pre",
          !open && needsCollapse && "tool-timeline-output-pre-clamped"
        )}
      >
        {value}
      </pre>
      {needsCollapse ? (
        <button
          onClick={() => setOpen((current) => !current)}
          className="tool-timeline-output-toggle inline-flex items-center gap-2 font-medium transition-colors hover:text-foreground focus-visible:outline-none"
        >
          <span>{`${lineCount} ${toolTimelineCopy.unit.line}`}</span>
          <span className="text-border/80">·</span>
          {open
            ? toolTimelineCopy.action.collapse
            : toolTimelineCopy.action.expand}
        </button>
      ) : null}
    </div>
  )
}

export function DetailList({
  entries,
}: {
  entries: { label: string; value: string | number | null | undefined }[]
}) {
  const visibleEntries = entries.filter(
    (entry) => entry.value != null && entry.value !== ""
  )
  if (visibleEntries.length === 0) return null

  return (
    <dl className="tool-timeline-detail-list">
      {visibleEntries.map((entry) => (
        <div key={entry.label} className="tool-timeline-detail-list-row">
          <dt className="tool-timeline-detail-list-label">{entry.label}</dt>
          <dd className="tool-timeline-detail-list-value">{entry.value}</dd>
        </div>
      ))}
    </dl>
  )
}

export function JsonBlock({ value }: { value: unknown }) {
  const [open, setOpen] = useState(false)

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="tool-timeline-json"
    >
      <CollapsibleTrigger className="tool-timeline-json-trigger flex w-full items-center justify-between text-left focus-visible:outline-none">
        <span className="tool-timeline-json-label">
          {toolTimelineCopy.section.content}
        </span>
        <span className="tool-timeline-json-hint">
          {open
            ? toolTimelineCopy.action.collapse
            : toolTimelineCopy.action.expand}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="tool-timeline-json-panel">
        <pre className="tool-timeline-json-pre">
          {JSON.stringify(value, null, 2)}
        </pre>
      </CollapsibleContent>
    </Collapsible>
  )
}

export function ToolDetailSection({
  title,
  children,
}: {
  title: string
  children: ReactNode
}) {
  const kind = resolveToolSectionKind(title)
  const tone = resolveToolSectionTone(kind)

  return (
    <section
      className="tool-timeline-detail-section"
      data-tool-detail-kind={kind}
      data-tool-detail-tone={tone}
    >
      <p className="tool-timeline-detail-title">{title}</p>
      <div className="tool-timeline-detail-body">{children}</div>
    </section>
  )
}

export function ToolInfoSection({
  title,
  hint,
  defaultOpen = true,
  children,
}: {
  title: string
  hint?: string
  defaultOpen?: boolean
  children: ReactNode
}) {
  const [open, setOpen] = useState(defaultOpen)
  const kind = resolveToolSectionKind(title)
  const tone = resolveToolSectionTone(kind)

  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="tool-timeline-info-section"
      data-tool-detail-kind={kind}
      data-tool-detail-tone={tone}
    >
      <CollapsibleTrigger className="tool-timeline-info-trigger flex w-full items-center justify-between gap-3 text-left focus-visible:outline-none">
        <span className="tool-timeline-info-title">{title}</span>
        <span className="tool-timeline-info-hint">
          {[
            hint,
            open
              ? toolTimelineCopy.action.collapse
              : toolTimelineCopy.action.expand,
          ]
            .filter(Boolean)
            .join(" · ")}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="tool-timeline-info-panel">
        <div className="tool-timeline-info-content">{children}</div>
      </CollapsibleContent>
    </Collapsible>
  )
}

export function ToolDetailSurface({
  children,
  className,
}: {
  children: ReactNode
  className?: string
}) {
  return (
    <div className={cn("tool-timeline-detail-surface", className)}>
      {children}
    </div>
  )
}
