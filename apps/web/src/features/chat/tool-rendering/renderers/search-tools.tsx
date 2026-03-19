import type { ReactNode } from "react"

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
