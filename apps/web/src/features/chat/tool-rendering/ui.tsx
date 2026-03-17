import { useState } from "react"
import type { ReactNode } from "react"

export function truncateInline(value: string, maxLength = 96): string {
  const compact = value.replace(/\s+/g, " ").trim()
  if (compact.length <= maxLength) return compact
  return `${compact.slice(0, maxLength - 1)}…`
}

export function formatScalar(value: unknown): string {
  if (typeof value === "string") return value
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value)
  }
  if (value == null) return "-"
  return JSON.stringify(value)
}

export function getStringValue(
  record: Record<string, unknown> | undefined,
  ...keys: string[]
): string | undefined {
  if (!record) return undefined
  for (const key of keys) {
    const value = record[key]
    if (typeof value === "string" && value.length > 0) {
      return value
    }
  }
  return undefined
}

export function getNumberValue(
  record: Record<string, unknown> | undefined,
  ...keys: string[]
): number | undefined {
  if (!record) return undefined
  for (const key of keys) {
    const value = record[key]
    if (typeof value === "number") {
      return value
    }
  }
  return undefined
}

export function getBooleanValue(
  record: Record<string, unknown> | undefined,
  ...keys: string[]
): boolean | undefined {
  if (!record) return undefined
  for (const key of keys) {
    const value = record[key]
    if (typeof value === "boolean") {
      return value
    }
  }
  return undefined
}

export function getArrayValue(
  record: Record<string, unknown> | undefined,
  key: string
): unknown[] {
  const value = record?.[key]
  return Array.isArray(value) ? value : []
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

  return (
    <div className="space-y-2">
      <pre
        className={`overflow-auto rounded-md border p-2 text-[12px] leading-relaxed whitespace-pre-wrap ${
          failed
            ? "border-destructive/20 bg-destructive/[0.04] text-destructive/90"
            : "border-border/30 bg-background/60 text-muted-foreground/80"
        } ${!open && needsCollapse ? "max-h-44" : ""}`}
      >
        {value}
      </pre>
      {needsCollapse ? (
        <button
          onClick={() => setOpen((current) => !current)}
          className="text-[11px] font-medium text-muted-foreground transition-colors hover:text-foreground"
        >
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
  const visibleEntries = entries.filter((entry) => entry.value != null && entry.value !== "")
  if (visibleEntries.length === 0) return null

  return (
    <dl className="divide-y divide-border/20 overflow-hidden rounded-md border border-border/30 bg-background/60">
      {visibleEntries.map((entry) => (
        <div
          key={entry.label}
          className="flex items-start justify-between gap-3 px-2.5 py-2"
        >
          <dt className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
            {entry.label}
          </dt>
          <dd className="text-right text-[12px] leading-5 text-foreground/80">
            {entry.value}
          </dd>
        </div>
      ))}
    </dl>
  )
}

export function JsonBlock({ value }: { value: unknown }) {
  return (
    <pre className="max-h-64 overflow-auto rounded-md border border-border/30 bg-background/60 p-2 text-[11px] leading-5 text-muted-foreground/80">
      {JSON.stringify(value, null, 2)}
    </pre>
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
    <div className="space-y-1.5">
      <p className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
        {title}
      </p>
      {children}
    </div>
  )
}
