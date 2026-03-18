import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  compactPath,
  createMetaBadge,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import { ExpandableOutput, ToolDetailSection } from "../ui"

export function createReadRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "read",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const path = getToolDisplayPath(data.toolName, data.details, args)
      return compactPath(path, 64)
    },
    renderMeta(data) {
      const offset = getNumberValue(data.arguments, "offset") ?? 0
      const linesRead = getNumberValue(data.details, "lines_read")

      if (linesRead == null) return null

      if (linesRead <= 0) {
        return <>{createMetaBadge("0L")}</>
      }

      const startLine = offset + 1
      const endLine = offset + linesRead

      return <>{createMetaBadge(`L${startLine}-${endLine}`)}</>
    },
    renderDetails(data) {
      const content = getStringValue(data.details, "diff") ?? data.outputContent

      if (!content) return null

      return (
        <ToolDetailSection title={data.succeeded ? "Content" : "Failure"}>
          <ExpandableOutput value={content} failed={!data.succeeded} />
        </ToolDetailSection>
      )
    },
  }
}

export function createWriteRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "write",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const path = getToolDisplayPath(data.toolName, data.details, args)
      return compactPath(path, 64)
    },
    renderMeta(data) {
      const lines = getNumberValue(data.details, "lines")
      return lines != null
        ? createMetaBadge(`+${lines}`, "text-emerald-500")
        : null
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

export function createEditRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "edit",
    renderTitle(data) {
      if (!data.succeeded) {
        return truncateInline(data.outputContent, 88)
      }

      const args = normalizeToolArguments(data.arguments)
      const path = getToolDisplayPath(data.toolName, data.details, args)
      return compactPath(path, 64)
    },
    renderMeta(data) {
      const added = getNumberValue(data.details, "added")
      const removed = getNumberValue(data.details, "removed")
      if (added == null && removed == null) return null
      return (
        <>
          {added != null
            ? createMetaBadge(`+${added}`, "text-emerald-500")
            : null}
          {removed != null
            ? createMetaBadge(`-${removed}`, "text-red-400")
            : null}
        </>
      )
    },
    renderDetails(data) {
      const content = getStringValue(data.details, "diff") ?? data.outputContent

      if (!content) return null

      return (
        <ToolDetailSection title={data.succeeded ? "Content" : "Failure"}>
          <ExpandableOutput value={content} failed={!data.succeeded} />
        </ToolDetailSection>
      )
    },
  }
}
