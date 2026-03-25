import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import {
  createMetaBadge,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import { PierrePatchDiffOutput } from "../ui"

type ApplyPatchOperation = {
  added: number
  directory: string | null
  displayPath: string
  fileName: string
  filePath: string
  kind: "added" | "modified" | "moved" | "removed"
  patch: string
  key: string
  removed: number
}

function splitDisplayPath(displayPath: string) {
  if (displayPath.includes(" → ")) {
    return {
      directory: null,
      fileName: displayPath,
    }
  }

  const normalized = displayPath.replace(/\\/g, "/")
  const lastSlashIndex = normalized.lastIndexOf("/")

  if (lastSlashIndex === -1) {
    return {
      directory: null,
      fileName: displayPath,
    }
  }

  return {
    directory: displayPath.slice(0, lastSlashIndex + 1),
    fileName: displayPath.slice(lastSlashIndex + 1),
  }
}

function splitFilePath(filePath: string) {
  const normalized = filePath.replace(/\\/g, "/")
  const lastSlashIndex = normalized.lastIndexOf("/")

  if (lastSlashIndex === -1) {
    return {
      directory: null,
      fileName: filePath,
    }
  }

  return {
    directory: filePath.slice(0, lastSlashIndex + 1),
    fileName: filePath.slice(lastSlashIndex + 1),
  }
}

function normalizeLineEndings(value: string) {
  return value.replace(/\r\n/g, "\n")
}

function buildHunkHeader(header: string, lines: string[]) {
  if (/^@@\s+-\d/.test(header)) return header

  let deletionCount = 0
  let additionCount = 0

  for (const line of lines) {
    if (line.startsWith("-")) {
      deletionCount += 1
      continue
    }

    if (line.startsWith("+")) {
      additionCount += 1
      continue
    }

    deletionCount += 1
    additionCount += 1
  }

  return `@@ -1,${deletionCount} +1,${additionCount} @@`
}

function countPatchChanges(lines: string[]) {
  let added = 0
  let removed = 0

  for (const line of lines) {
    if (line.startsWith("+++ ") || line.startsWith("--- ")) continue

    if (line.startsWith("+")) {
      added += 1
      continue
    }

    if (line.startsWith("-")) {
      removed += 1
    }
  }

  return { added, removed }
}

function convertApplyPatchBody(bodyLines: string[]) {
  const output: string[] = []
  let currentHeader: string | null = null
  let currentLines: string[] = []

  function flushCurrentHunk() {
    if (currentHeader == null) return
    output.push(buildHunkHeader(currentHeader, currentLines), ...currentLines)
    currentHeader = null
    currentLines = []
  }

  for (const line of bodyLines) {
    if (line.startsWith("@@")) {
      flushCurrentHunk()
      currentHeader = line.trim()
      continue
    }

    currentLines.push(line)
  }

  if (currentHeader == null) {
    if (currentLines.length > 0) {
      output.push(buildHunkHeader("@@", currentLines), ...currentLines)
    }
  } else {
    flushCurrentHunk()
  }

  return output
}

function getPatchDisplayTitle(patch: string) {
  const operations = toPatchOperations(patch)
  const firstOperation = operations[0]

  return firstOperation?.displayPath ?? null
}

function getPatchDisplaySubtitle(patch: string) {
  const operations = toPatchOperations(patch)
  const firstOperation = operations[0]

  if (!firstOperation) return null
  if (operations.length === 1) return firstOperation.displayPath

  return `${firstOperation.displayPath} +${operations.length - 1} files`
}

function convertOperationToUnifiedDiff(
  kind: "update" | "add" | "delete",
  filePath: string,
  bodyLines: string[],
  moveToPath?: string
): Omit<ApplyPatchOperation, "key"> {
  const targetPath = moveToPath?.trim() || filePath
  const oldPath = kind === "add" ? "/dev/null" : `a/${filePath}`
  const newPath = kind === "delete" ? "/dev/null" : `b/${targetPath}`
  const diffTarget = `b/${targetPath}`
  const convertedBody = convertApplyPatchBody(bodyLines)
  const changeStats = countPatchChanges(convertedBody)
  const displayPath =
    targetPath === filePath ? filePath : `${filePath} → ${targetPath}`
  const operationKind: ApplyPatchOperation["kind"] =
    kind === "add"
      ? "added"
      : kind === "delete"
        ? "removed"
        : moveToPath?.trim()
          ? "moved"
          : "modified"
  const pathParts =
    operationKind === "moved"
      ? {
          directory: null,
          fileName: `${splitFilePath(filePath).fileName} → ${splitFilePath(targetPath).fileName}`,
        }
      : splitDisplayPath(displayPath)
  const directory =
    operationKind === "moved"
      ? (() => {
          const fromDirectory = splitFilePath(filePath).directory
          const toDirectory = splitFilePath(targetPath).directory

          if (fromDirectory === toDirectory) {
            return fromDirectory
          }

          return `${fromDirectory ?? ""} → ${toDirectory ?? ""}`
        })()
      : pathParts.directory

  return {
    added: changeStats.added,
    directory,
    displayPath,
    fileName: pathParts.fileName,
    filePath: targetPath,
    kind: operationKind,
    patch: [
      `diff --git a/${filePath} ${diffTarget}`,
      `--- ${oldPath}`,
      `+++ ${newPath}`,
      ...convertedBody,
    ].join("\n"),
    removed: changeStats.removed,
  }
}

function renderApplyPatchOperationList(operations: ApplyPatchOperation[]) {
  return (
    <div className="tool-timeline-patch-list">
      {operations.map((entry) => (
        <details
          key={entry.key}
          className="tool-timeline-patch-item"
          data-kind={entry.kind}
        >
          <summary className="tool-timeline-patch-summary">
            <span
              className="tool-timeline-patch-path"
              title={entry.displayPath}
            >
              <span className="tool-timeline-patch-filename">
                {entry.fileName}
                {entry.directory ? (
                  <span className="tool-timeline-patch-directory">
                    {` \u202A${entry.directory}\u202C`}
                  </span>
                ) : null}
              </span>
            </span>
            <span className="tool-timeline-patch-summary-meta">
              <span className="tool-timeline-patch-stats">
                <span className="tool-timeline-patch-stat text-emerald-400">
                  +{entry.added}
                </span>
                <span className="tool-timeline-patch-stat text-red-400">
                  -{entry.removed}
                </span>
              </span>
              <span className="tool-timeline-patch-chevron" aria-hidden="true">
                ›
              </span>
            </span>
          </summary>
          <div className="tool-timeline-patch-body">
            <PierrePatchDiffOutput patch={entry.patch} />
          </div>
        </details>
      ))}
    </div>
  )
}

function toPatchOperations(value: string): ApplyPatchOperation[] {
  const normalized = normalizeLineEndings(value)

  if (normalized.startsWith("diff --git ")) {
    const lines = normalized.split("\n")
    const firstDiffLine = lines.find((line) => line.startsWith("diff --git "))
    const filePath =
      firstDiffLine?.split(" ")[2]?.replace(/^a\//, "") ?? "Patch"
    const changeStats = countPatchChanges(lines)

    return [
      {
        added: changeStats.added,
        directory: null,
        displayPath: filePath,
        fileName: filePath,
        filePath,
        kind: "modified",
        key: "patch-0",
        patch: normalized,
        removed: changeStats.removed,
      },
    ]
  }

  const lines = normalized
    .split("\n")
    .filter((line) => line !== "*** Begin Patch" && line !== "*** End Patch")

  const operations: ApplyPatchOperation[] = []
  let currentKind: "update" | "add" | "delete" | null = null
  let currentPath: string | null = null
  let moveToPath: string | undefined
  let bodyLines: string[] = []

  function flushCurrentOperation() {
    if (currentKind == null || currentPath == null) return

    operations.push({
      key: `${currentKind}:${currentPath}:${operations.length}`,
      ...convertOperationToUnifiedDiff(
        currentKind,
        currentPath,
        bodyLines,
        moveToPath
      ),
    })

    currentKind = null
    currentPath = null
    moveToPath = undefined
    bodyLines = []
  }

  for (const line of lines) {
    if (line.startsWith("*** Update File:")) {
      flushCurrentOperation()
      currentKind = "update"
      currentPath = line.slice("*** Update File:".length).trim()
      continue
    }

    if (line.startsWith("*** Add File:")) {
      flushCurrentOperation()
      currentKind = "add"
      currentPath = line.slice("*** Add File:".length).trim()
      continue
    }

    if (line.startsWith("*** Delete File:")) {
      flushCurrentOperation()
      currentKind = "delete"
      currentPath = line.slice("*** Delete File:".length).trim()
      continue
    }

    if (line.startsWith("*** Move to:")) {
      moveToPath = line.slice("*** Move to:".length).trim()
      continue
    }

    if (currentKind != null) {
      bodyLines.push(line)
    }
  }

  flushCurrentOperation()
  return operations
}

export function createApplyPatchRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "ApplyPatch",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const patch = getStringValue(args, "patch", "patchText")
      if (!patch) {
        const fallbackPath = getToolDisplayPath(
          data.toolName,
          data.details,
          args
        )
        return fallbackPath
          ? truncateInline(fallbackPath, 96)
          : toolTimelineCopy.toolName.patch
      }
      const filePath = getPatchDisplayTitle(patch)

      return filePath
        ? truncateInline(filePath, 96)
        : toolTimelineCopy.toolName.patch
    },
    renderSubtitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const patch = getStringValue(args, "patch", "patchText")
      if (!patch) return null

      const subtitle = getPatchDisplaySubtitle(patch)
      return subtitle ? truncateInline(subtitle, 96) : null
    },
    renderMeta(data) {
      const linesAdded = getNumberValue(data.details, "lines_added")
      const linesRemoved = getNumberValue(data.details, "lines_removed")
      if (linesAdded == null && linesRemoved == null) return null
      return (
        <>
          {linesAdded != null
            ? createMetaBadge(`+${linesAdded}`, "text-emerald-400")
            : null}
          {linesRemoved != null
            ? createMetaBadge(`-${linesRemoved}`, "text-red-400")
            : null}
        </>
      )
    },
    renderDetails(data) {
      const args = normalizeToolArguments(data.arguments)
      const content =
        getStringValue(args, "patch", "patchText") ?? data.outputContent

      if (!content) return null

      const patches = toPatchOperations(content)

      if (patches.length === 0) return null

      return renderApplyPatchOperationList(patches)
    },
  }
}
