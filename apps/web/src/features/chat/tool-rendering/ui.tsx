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
        className={`overflow-auto rounded-xl border px-3 py-2.5 text-[12px] leading-relaxed whitespace-pre-wrap ${
          failed
            ? "border-destructive/20 bg-destructive/[0.04] text-destructive/90"
            : "border-border/30 bg-background/80 text-muted-foreground/85"
        } ${!open && needsCollapse ? "max-h-52" : ""}`}
      >
        {value}
      </pre>
      {needsCollapse ? (
        <button
          onClick={() => setOpen((current) => !current)}
          className="inline-flex items-center gap-2 text-[11px] font-medium text-muted-foreground transition-colors hover:text-foreground"
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
    <dl className="overflow-hidden rounded-xl border border-border/30 bg-background/80">
      {visibleEntries.map((entry) => (
        <div
          key={entry.label}
          className="grid grid-cols-[minmax(88px,132px)_1fr] items-start gap-3 border-b border-border/20 px-3 py-2.5 last:border-b-0"
        >
          <dt className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
            {entry.label}
          </dt>
          <dd className="text-[12px] leading-5 break-words text-foreground/85">
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
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="overflow-hidden rounded-xl border border-border/30 bg-background/80"
    >
      <CollapsibleTrigger className="flex w-full items-center justify-between border-b border-border/20 bg-muted/[0.12] px-3 py-2.5 text-left">
        <span className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
          Json
        </span>
        <span className="text-[11px] text-muted-foreground">
          {open ? "Collapse" : "Expand"}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="p-3">
        <pre className="max-h-72 overflow-auto rounded-xl border border-border/20 bg-background px-3 py-2.5 text-[11px] leading-5 text-foreground/85">
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
      <p className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
        {title}
      </p>
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
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="overflow-hidden rounded-xl border border-border/30 bg-muted/[0.08]"
    >
      <CollapsibleTrigger className="flex w-full items-center justify-between gap-3 border-b border-border/20 px-3 py-2.5 text-left">
        <span className="text-[12px] font-medium text-foreground/90">
          {title}
        </span>
        <span className="text-[11px] text-muted-foreground">
          {[hint, open ? "Collapse" : "Expand"].filter(Boolean).join(" · ")}
        </span>
      </CollapsibleTrigger>
      <CollapsibleContent className="p-3">{children}</CollapsibleContent>
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
    <div
      className={cn(
        "space-y-3 rounded-xl border border-border/30 bg-background/70 p-3",
        className
      )}
    >
      {children}
    </div>
  )
}
