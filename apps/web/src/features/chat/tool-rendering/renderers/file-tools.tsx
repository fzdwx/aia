import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import {
  compactPath,
  createMetaBadge,
  getNumberValue,
  getStringValue,
} from "../helpers"
import { ExpandableOutput, ToolDetailSection } from "../ui"

function buildEditDiffFromArguments(
  data: Parameters<ToolRenderer["renderDetails"]>[0]
) {
  const args = normalizeToolArguments(data.arguments)
  const oldString = getStringValue(args, "old_string")
  const newString = getStringValue(args, "new_string")

  if (!oldString && !newString) return null

  const removed = oldString
    ? oldString.split("\n").map((line) => `-${line}`)
    : []
  const added = newString ? newString.split("\n").map((line) => `+${line}`) : []
  const lines = [...removed, ...added]

  return lines.length > 0 ? lines.join("\n") : null
}

export function createReadRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "Read",
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
      const content =
        getStringValue(data.details, "diff") ??
        buildEditDiffFromArguments(data) ??
        data.outputContent

      if (!content) return null

      return (
        <ToolDetailSection
          title={
            data.succeeded
              ? toolTimelineCopy.section.content
              : toolTimelineCopy.section.failure
          }
        >
          <ExpandableOutput value={content} failed={!data.succeeded} />
        </ToolDetailSection>
      )
    },
  }
}

export function createWriteRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "Write",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const path = getToolDisplayPath(data.toolName, data.details, args)
      return compactPath(path, 64)
    },
    renderMeta(data) {
      const lines = getNumberValue(data.details, "lines")
      return lines != null
        ? createMetaBadge(`+${lines}`, "text-emerald-400")
        : null
    },
    renderDetails(data) {
      const args = normalizeToolArguments(data.arguments)
      const content =
        getStringValue(args, "content", "contents", "text", "value") ??
        data.outputContent

      if (!content) return null

      return <ExpandableOutput value={content} failed={!data.succeeded} />
    },
  }
}

export function createEditRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "Edit",
    renderTitle(data) {
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
            ? createMetaBadge(`+${added}`, "text-emerald-400")
            : null}
          {removed != null
            ? createMetaBadge(`-${removed}`, "text-red-400")
            : null}
        </>
      )
    },
    renderDetails(data) {
      const content =
        getStringValue(data.details, "diff") ??
        buildEditDiffFromArguments(data) ??
        data.outputContent

      if (!content) return null

      return <ExpandableOutput value={content} failed={!data.succeeded} />
    },
  }
}
