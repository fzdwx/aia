import {
  type FileContents,
  MultiFileDiff,
  PatchDiff,
} from "@pierre/diffs/react"
import { useMemo, useState } from "react"
import type { CSSProperties, ReactNode } from "react"

import { useTheme } from "@/components/theme-provider"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { cn } from "@/lib/utils"

import { toolTimelineCopy } from "../tool-timeline-copy"

const PIERRE_DIFF_UNSAFE_CSS = `
:host {
  --diffs-bg: var(--aia-diff-surface);
}
`

const PIERRE_DIFF_HOST_STYLE: CSSProperties & Record<`--${string}`, string> = {
  background: "var(--aia-diff-surface)",
  "--aia-diff-surface":
    "color-mix(in oklch, var(--workspace-surface-soft) 84%, var(--background))",
  "--diffs-bg": "var(--aia-diff-surface)",
  "--diffs-bg-buffer-override": "var(--aia-diff-surface)",
  "--diffs-bg-hover-override": "var(--aia-diff-surface)",
  "--diffs-fg-number-override": "var(--text-weak)",
  "--diffs-fg-number-addition-override": "var(--text-weak)",
  "--diffs-fg-number-deletion-override": "var(--text-weak)",
  "--diffs-fg-conflict-marker-override": "var(--text-weak)",
  "--shiki-background": "var(--aia-diff-surface)",
  "--diffs-font-family": "var(--font-mono)",
  "--diffs-font-size": "var(--font-size-meta)",
  "--diffs-line-height": "24px",
  "--diffs-tab-size": "2",
  "--diffs-header-font-family": "var(--font-sans)",
  "--diffs-gap-block": "0",
  "--diffs-min-number-column-width": "4ch",
}

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

function usePierreDiffOptions(diffStyle: "unified" | "split") {
  const { resolvedTheme } = useTheme()

  return useMemo(() => {
    const lineDiffType: "word-alt" | "none" =
      diffStyle === "split" ? "word-alt" : "none"

    return {
      theme: { dark: "pierre-dark", light: "pierre-light" },
      themeType: resolvedTheme,
      diffStyle,
      diffIndicators: "bars" as const,
      lineHoverHighlight: "both" as const,
      disableBackground: false,
      expansionLineCount: 20,
      hunkSeparators: "line-info-basic" as const,
      lineDiffType,
      maxLineDiffLength: 1000,
      maxLineLengthForHighlighting: 1000,
      unsafeCSS: PIERRE_DIFF_UNSAFE_CSS,
      overflow: "wrap" as const,
      disableFileHeader: true,
    }
  }, [diffStyle, resolvedTheme])
}

function createContentCacheKey(
  fileName: string,
  side: "old" | "new",
  content: string
) {
  let hash = 2166136261

  for (let index = 0; index < content.length; index += 1) {
    hash ^= content.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }

  return `${fileName}:${side}:${content.length}:${(hash >>> 0).toString(16)}`
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

export function PierrePatchDiffOutput({ patch }: { patch: string }) {
  const options = usePierreDiffOptions("unified")

  return (
    <PatchDiff
      patch={patch}
      options={options}
      className="tool-timeline-pierre-root tool-timeline-pierre-root-patch"
      style={PIERRE_DIFF_HOST_STYLE}
    />
  )
}

export function PierreMultiFileDiffOutput({
  fileName,
  oldContent,
  newContent,
  diffStyle,
}: {
  fileName: string
  oldContent: string
  newContent: string
  diffStyle: "unified" | "split"
}) {
  const options = usePierreDiffOptions(diffStyle)
  const oldFile = useMemo<FileContents>(
    () => ({
      name: fileName,
      contents: oldContent,
      cacheKey: createContentCacheKey(fileName, "old", oldContent),
    }),
    [fileName, oldContent]
  )
  const newFile = useMemo<FileContents>(
    () => ({
      name: fileName,
      contents: newContent,
      cacheKey: createContentCacheKey(fileName, "new", newContent),
    }),
    [fileName, newContent]
  )

  return (
    <MultiFileDiff
      oldFile={oldFile}
      newFile={newFile}
      options={options}
      className="tool-timeline-pierre-root tool-timeline-pierre-root-multi"
      style={PIERRE_DIFF_HOST_STYLE}
    />
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
