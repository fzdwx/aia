import { useState } from "react"
import type { ReactNode } from "react"

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { cn } from "@/lib/utils"

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
    <div className="space-y-2">
      <pre
        className={`text-caption overflow-auto whitespace-pre-wrap ${
          failed ? "text-destructive/90" : "text-muted-foreground/85"
        } ${!open && needsCollapse ? "max-h-52" : ""}`}
      >
        {value}
      </pre>
      {needsCollapse ? (
        <button
          onClick={() => setOpen((current) => !current)}
          className="text-meta inline-flex items-center gap-2 font-medium text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none"
        >
          <span>{`${lineCount} lines`}</span>
          <span className="text-border/80">·</span>
          {open ? "Collapse" : "Expand"}
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
    <dl className="space-y-2">
      {visibleEntries.map((entry) => (
        <div
          key={entry.label}
          className="grid grid-cols-[minmax(80px,120px)_1fr] items-start gap-3"
        >
          <dt className="text-meta font-medium text-foreground/70">
            {entry.label}
          </dt>
          <dd className="text-caption break-words text-foreground/85">
            {entry.value}
          </dd>
        </div>
      ))}
    </dl>
  )
}

export function JsonBlock({ value }: { value: unknown }) {
  const [open, setOpen] = useState(false)

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="overflow-hidden">
      <CollapsibleTrigger className="flex w-full items-center justify-between py-1 text-left focus-visible:outline-none">
        <span className="text-meta font-medium text-foreground/70">JSON</span>
        <span className="text-meta text-muted-foreground">
          {open ? "Collapse" : "Expand"}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="pt-1">
        <pre className="text-meta max-h-72 overflow-auto text-foreground/85">
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
  return (
    <div className="space-y-2">
      <p className="text-meta font-medium text-foreground/70">{title}</p>
      {children}
    </div>
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

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="overflow-hidden">
      <CollapsibleTrigger className="flex w-full items-center justify-between gap-3 py-1 text-left focus-visible:outline-none">
        <span className="text-ui font-medium text-foreground/90">{title}</span>
        <span className="text-meta text-muted-foreground">
          {[hint, open ? "Collapse" : "Expand"].filter(Boolean).join(" · ")}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="pt-1">{children}</CollapsibleContent>
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
    <div className={cn("space-y-3 pt-1.5 pl-4", className)}>{children}</div>
  )
}
