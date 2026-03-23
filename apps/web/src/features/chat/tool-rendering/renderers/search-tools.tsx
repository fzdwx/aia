import type { ReactNode } from "react"

import { Code2, ExternalLink, Globe2 } from "lucide-react"

import { normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  createMetaBadge,
  getBooleanValue,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import { ExpandableOutput, ToolDetailSection } from "../ui"

type SearchPreview = {
  title: string | null
  url: string | null
  snippet: string | null
}

function renderSearchMeta(data: {
  details?: Record<string, unknown>
}): ReactNode | null {
  const matches = getNumberValue(data.details, "matches")
  const returned = getNumberValue(data.details, "returned")
  const truncated = getBooleanValue(data.details, "truncated")

  if (matches == null) return null
  if (truncated && returned != null) {
    return createMetaBadge(
      `${matches} matches (showing ${returned})`,
      "text-amber-600/80"
    )
  }
  return createMetaBadge(`${matches} matches`)
}

function renderCodeSearchMeta(data: {
  arguments: Record<string, unknown>
  details?: Record<string, unknown>
}): ReactNode | null {
  const args = normalizeToolArguments(data.arguments)
  const tokensNum = getNumberValue(args, "tokensNum", "tokens_num")
  const resultFound = getBooleanValue(data.details, "result_found")

  if (tokensNum == null && resultFound == null) return null

  return (
    <>
      {tokensNum != null
        ? createMetaBadge(`${tokensNum.toLocaleString()} tok`)
        : null}
      {resultFound === false
        ? createMetaBadge("no result", "text-amber-600/80")
        : null}
    </>
  )
}

function formatSearchModeLabel(value: string): string {
  const normalized = value.trim().toLowerCase()
  if (!normalized) return value
  return normalized.charAt(0).toUpperCase() + normalized.slice(1)
}

function renderWebSearchMeta(data: {
  arguments: Record<string, unknown>
  details?: Record<string, unknown>
}): ReactNode | null {
  const args = normalizeToolArguments(data.arguments)
  const numResults = getNumberValue(args, "numResults", "num_results")
  const searchType = getStringValue(args, "type")
  const livecrawl = getStringValue(args, "livecrawl")
  const resultFound = getBooleanValue(data.details, "result_found")

  if (numResults == null && !searchType && !livecrawl && resultFound == null) {
    return null
  }

  return (
    <>
      {numResults != null ? createMetaBadge(`${numResults} results`) : null}
      {searchType && searchType !== "auto"
        ? createMetaBadge(formatSearchModeLabel(searchType))
        : null}
      {livecrawl === "preferred"
        ? createMetaBadge("Live crawl", "text-sky-600/80")
        : null}
      {resultFound === false
        ? createMetaBadge("no result", "text-amber-600/80")
        : null}
    </>
  )
}

function extractSearchPreview(content: string): SearchPreview {
  const lines = content
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)

  if (lines.length === 0) {
    return { title: null, url: null, snippet: null }
  }

  const title = lines[0] ?? null
  const urlLine = lines.find(
    (line, index) => index > 0 && /^https?:\/\//.test(line)
  )
  const urlIndex = urlLine ? lines.indexOf(urlLine) : -1
  const snippetLines =
    urlIndex >= 0 ? lines.slice(urlIndex + 1, urlIndex + 5) : lines.slice(1, 5)
  const snippet = snippetLines.length > 0 ? snippetLines.join("\n") : null

  return {
    title,
    url: urlLine ?? null,
    snippet,
  }
}

function getHostname(url: string | null): string | null {
  if (!url) return null

  try {
    return new URL(url).hostname.replace(/^www\./, "")
  } catch {
    return null
  }
}

function renderSearchResultCard({
  kind,
  preview,
}: {
  kind: "code" | "web"
  preview: SearchPreview
}) {
  const hostname = getHostname(preview.url)
  const isCode = kind === "code"

  return (
    <div className="overflow-hidden rounded-lg border border-border/35 bg-gradient-to-br from-background via-background to-muted/20 shadow-sm">
      <div className="flex items-start justify-between gap-3 border-b border-border/20 px-3 py-2.5">
        <div className="min-w-0 space-y-1">
          <div className="flex items-center gap-2 text-[11px] text-muted-foreground/80">
            <span
              className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 font-medium ${
                isCode
                  ? "bg-violet-500/10 text-violet-500"
                  : "bg-sky-500/10 text-sky-500"
              }`}
            >
              {isCode ? (
                <Code2 className="size-3" />
              ) : (
                <Globe2 className="size-3" />
              )}
              {isCode ? "Code match" : "Web result"}
            </span>
            {hostname ? (
              <span className="truncate rounded-full border border-border/30 px-2 py-0.5 text-[10px] tracking-wide text-muted-foreground/70 uppercase">
                {hostname}
              </span>
            ) : null}
          </div>
          {preview.title ? (
            <div className="text-[13px] leading-5 font-medium text-foreground/90">
              {preview.title}
            </div>
          ) : null}
        </div>
        {preview.url ? (
          <a
            href={preview.url}
            target="_blank"
            rel="noreferrer"
            className="shrink-0 rounded-md border border-border/30 bg-background/70 p-1.5 text-muted-foreground transition-colors hover:text-foreground"
            aria-label="Open result"
          >
            <ExternalLink className="size-3.5" />
          </a>
        ) : null}
      </div>
      <div className="space-y-2 px-3 py-2.5">
        {preview.url ? (
          <div className="truncate text-[11px] leading-4 text-muted-foreground/75">
            {preview.url}
          </div>
        ) : null}
        {preview.snippet ? (
          <p className="text-[12px] leading-5 whitespace-pre-wrap text-foreground/75">
            {preview.snippet}
          </p>
        ) : null}
      </div>
    </div>
  )
}

function renderSearchDetails(data: {
  outputContent: string
  succeeded: boolean
  kind: "code" | "web"
}): ReactNode | null {
  if (!data.outputContent) return null

  const preview = extractSearchPreview(data.outputContent)
  const hasPreview = preview.title || preview.url || preview.snippet

  return (
    <div className="space-y-2.5">
      {data.succeeded && hasPreview ? (
        <ToolDetailSection title="Top Result">
          {renderSearchResultCard({ kind: data.kind, preview })}
        </ToolDetailSection>
      ) : null}
      <ToolDetailSection title={data.succeeded ? "Content" : "Failure"}>
        <ExpandableOutput value={data.outputContent} failed={!data.succeeded} />
      </ToolDetailSection>
    </div>
  )
}

export function createGlobRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "glob",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const pattern = getStringValue(args, "pattern")
      const path = getStringValue(args, "path")
      return [pattern, path ? `in ${path}` : ""].filter(Boolean).join(" — ")
    },
    renderMeta(data) {
      return renderSearchMeta(data)
    },
    renderDetails(data) {
      if (!data.outputContent) return null

      return (
        <ToolDetailSection title={data.succeeded ? "Content" : "Failure"}>
          <ExpandableOutput
            value={data.outputContent}
            failed={!data.succeeded}
          />
        </ToolDetailSection>
      )
    },
  }
}

export function createGrepRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "grep",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const pattern = getStringValue(args, "pattern")
      return pattern ? truncateInline(pattern, 48) : ""
    },
    renderMeta(data) {
      return renderSearchMeta(data)
    },
    renderDetails(data) {
      if (!data.outputContent) return null

      return (
        <ToolDetailSection title={data.succeeded ? "Content" : "Failure"}>
          <ExpandableOutput
            value={data.outputContent}
            failed={!data.succeeded}
          />
        </ToolDetailSection>
      )
    },
  }
}

export function createCodeSearchRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "codesearch",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const query = getStringValue(args, "query")
      return query ? truncateInline(query, 64) : "programming context"
    },
    renderMeta(data) {
      return renderCodeSearchMeta(data)
    },
    renderDetails(data) {
      return renderSearchDetails({ ...data, kind: "code" })
    },
  }
}

export function createWebSearchRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "websearch",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const query = getStringValue(args, "query")
      return query ? truncateInline(query, 64) : "web search"
    },
    renderMeta(data) {
      return renderWebSearchMeta(data)
    },
    renderDetails(data) {
      return renderSearchDetails({ ...data, kind: "web" })
    },
  }
}
