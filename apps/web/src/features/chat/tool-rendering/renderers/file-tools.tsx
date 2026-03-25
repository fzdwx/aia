import {
  getFileDisplayParts,
  getToolDisplayPath,
  normalizeToolArguments,
} from "@/lib/tool-display"

import { formatReadLineRange } from "../../read-range"
import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import {
  compactPath,
  createMetaBadge,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import {
  ExpandableOutput,
  PierreMultiFileDiffOutput,
  PierrePatchDiffOutput,
  ToolDetailSection,
} from "../ui"

function buildEditPatchFromDetails(
  data: Parameters<ToolRenderer["renderDetails"]>[0],
  fileName: string
) {
  const diff = getStringValue(data.details, "diff")
  if (!diff) return null

  if (diff.startsWith("diff --git ")) return diff

  return [
    `diff --git a/${fileName} b/${fileName}`,
    `--- a/${fileName}`,
    `+++ b/${fileName}`,
    diff,
  ].join("\n")
}

function buildEditContentsFromArguments(
  data: Parameters<ToolRenderer["renderDetails"]>[0]
) {
  const args = normalizeToolArguments(data.arguments)
  const oldString = getStringValue(args, "old_string")
  const newString = getStringValue(args, "new_string")

  if (!oldString && !newString) return null

  return {
    oldContent: oldString ?? "",
    newContent: newString ?? "",
  }
}

function getToolFileName(data: Parameters<ToolRenderer["renderDetails"]>[0]) {
  const args = normalizeToolArguments(data.arguments)

  return (
    getToolDisplayPath(data.toolName, data.details, args) ??
    getStringValue(args, "file_path", "path") ??
    `${data.toolName.toLowerCase()}.txt`
  )
}

function buildFileToolTitle(path: string) {
  const { fileName, directory } = getFileDisplayParts(path)

  if (!directory) return truncateInline(fileName, 40)

  return `${truncateInline(fileName, 40)} ${compactPath(directory, 48)}`
}

function buildWriteContentsFromArguments(
  data: Parameters<ToolRenderer["renderDetails"]>[0]
) {
  const args = normalizeToolArguments(data.arguments)
  const content = getStringValue(args, "content", "contents", "text", "value")

  if (!content) return null

  return {
    oldContent: "",
    newContent: content,
  }
}

export function createReadRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "Read",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const path = getToolDisplayPath(data.toolName, data.details, args)
      return buildFileToolTitle(path)
    },
    renderMeta(data) {
      const range = formatReadLineRange({
        offset: getNumberValue(data.arguments, "offset"),
        limit: getNumberValue(data.arguments, "limit"),
        linesRead: getNumberValue(data.details, "lines_read"),
        totalLines: getNumberValue(data.details, "total_lines"),
      })

      return range ? <>{createMetaBadge(range)}</> : null
    },
    renderDetails(data) {
      const content = getStringValue(data.details, "diff") ?? data.outputContent

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
      return buildFileToolTitle(path)
    },
    renderMeta(data) {
      const lines = getNumberValue(data.details, "lines")
      return lines != null
        ? createMetaBadge(`+${lines}`, "text-emerald-400")
        : null
    },
    renderDetails(data) {
      const fileName = getToolFileName(data)
      const contents = buildWriteContentsFromArguments(data)

      if (!contents) return null

      return (
        <PierreMultiFileDiffOutput
          fileName={fileName}
          oldContent={contents.oldContent}
          newContent={contents.newContent}
          diffStyle="unified"
        />
      )
    },
  }
}

export function createEditRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "Edit",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const path = getToolDisplayPath(data.toolName, data.details, args)
      return buildFileToolTitle(path)
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
      const fileName = getToolFileName(data)
      const patch = buildEditPatchFromDetails(data, fileName)
      const contents = buildEditContentsFromArguments(data)

      if (patch) {
        return <PierrePatchDiffOutput patch={patch} />
      }

      if (contents) {
        return (
          <PierreMultiFileDiffOutput
            fileName={fileName}
            oldContent={contents.oldContent}
            newContent={contents.newContent}
            diffStyle="split"
          />
        )
      }

      if (!data.outputContent) return null

      return (
        <pre
          className="tool-timeline-output-pre"
          data-failed={!data.succeeded || undefined}
        >
          {data.outputContent}
        </pre>
      )
    },
  }
}
