import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  DetailList,
  ExpandableOutput,
  ToolDetailSection,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../ui"

export function createReadRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "read",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const primary = getToolDisplayPath(data.toolName, data.details, args)
      const offset = getNumberValue(args, "offset")
      const limit = getNumberValue(args, "limit")
      const segments = [primary]
      if (offset != null || limit != null) {
        const parts = []
        if (offset != null) parts.push(`from ${offset}`)
        if (limit != null) parts.push(`limit ${limit}`)
        segments.push(parts.join(" · "))
      }
      return segments.filter(Boolean).join(" — ")
    },
    renderDetails(data) {
      return (
        <div className="space-y-2.5">
          <ToolDetailSection title="Read">
            <DetailList
              entries={[
                { label: "File", value: getStringValue(data.details, "file_path") },
                { label: "Lines", value: getNumberValue(data.details, "lines_read") },
                {
                  label: "Total lines",
                  value: getNumberValue(data.details, "total_lines"),
                },
              ]}
            />
          </ToolDetailSection>
          {data.outputContent ? (
            <ToolDetailSection title="Content">
              <ExpandableOutput value={data.outputContent} failed={!data.succeeded} />
            </ToolDetailSection>
          ) : null}
        </div>
      )
    },
  }
}

export function createWriteRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "write",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const primary = getToolDisplayPath(data.toolName, data.details, args)
      const content = getStringValue(args, "content")
      return [primary, content ? truncateInline(content, 72) : ""]
        .filter(Boolean)
        .join(" — ")
    },
    renderDetails(data) {
      return (
        <div className="space-y-2.5">
          <ToolDetailSection title="Write">
            <DetailList
              entries={[
                { label: "File", value: getStringValue(data.details, "file_path") },
                { label: "Lines", value: getNumberValue(data.details, "lines") },
              ]}
            />
          </ToolDetailSection>
          {data.outputContent ? (
            <ToolDetailSection title="Outcome">
              <ExpandableOutput value={data.outputContent} failed={!data.succeeded} />
            </ToolDetailSection>
          ) : null}
        </div>
      )
    },
  }
}

export function createEditRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "edit",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const primary = getToolDisplayPath(data.toolName, data.details, args)
      const oldString = getStringValue(args, "old_string")
      const newString = getStringValue(args, "new_string")
      const change = [
        oldString ? `replace ${truncateInline(oldString, 32)}` : "",
        newString ? `with ${truncateInline(newString, 32)}` : "",
      ]
        .filter(Boolean)
        .join(" ")
      return [primary, change].filter(Boolean).join(" — ")
    },
    renderDetails(data) {
      return (
        <div className="space-y-2.5">
          <ToolDetailSection title="Edit">
            <DetailList
              entries={[
                { label: "File", value: getStringValue(data.details, "file_path") },
                { label: "Added", value: getNumberValue(data.details, "added") },
                { label: "Removed", value: getNumberValue(data.details, "removed") },
              ]}
            />
          </ToolDetailSection>
          {data.outputContent ? (
            <ToolDetailSection title="Outcome">
              <ExpandableOutput value={data.outputContent} failed={!data.succeeded} />
            </ToolDetailSection>
          ) : null}
        </div>
      )
    },
  }
}
