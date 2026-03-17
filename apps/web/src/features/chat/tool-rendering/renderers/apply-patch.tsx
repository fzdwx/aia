import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  DetailList,
  ExpandableOutput,
  ToolDetailSection,
  getArrayValue,
  getNumberValue,
  getStringValue,
  truncateInline,
} from "../ui"

export function createApplyPatchRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "apply_patch",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const patch = getStringValue(args, "patch", "patchText")
      if (!patch) return getToolDisplayPath(data.toolName, data.details, args)
      const firstOperation = patch
        .split("\n")
        .find(
          (line) =>
            line.startsWith("*** Update File:") ||
            line.startsWith("*** Add File:") ||
            line.startsWith("*** Delete File:")
        )
      return truncateInline(firstOperation ?? "apply patch", 120)
    },
    renderDetails(data) {
      const files = getArrayValue(data.details, "files")
      return (
        <div className="space-y-2.5">
          <ToolDetailSection title="Summary">
            <DetailList
              entries={[
                { label: "Updated", value: getNumberValue(data.details, "files_updated") },
                { label: "Added", value: getNumberValue(data.details, "files_added") },
                { label: "Deleted", value: getNumberValue(data.details, "files_deleted") },
                { label: "Moved", value: getNumberValue(data.details, "files_moved") },
                { label: "Lines added", value: getNumberValue(data.details, "lines_added") },
                {
                  label: "Lines removed",
                  value: getNumberValue(data.details, "lines_removed"),
                },
              ]}
            />
          </ToolDetailSection>
          {files.length > 0 ? (
            <ToolDetailSection title="Files">
              <div className="space-y-2">
                {files.map((file, index) => {
                  const fileRecord =
                    file && typeof file === "object" && !Array.isArray(file)
                      ? (file as Record<string, unknown>)
                      : undefined
                  if (!fileRecord) return null
                  const filePath =
                    getStringValue(fileRecord, "file_path") ?? `file ${index + 1}`
                  const kind = getStringValue(fileRecord, "kind")
                  const moveTo = getStringValue(fileRecord, "move_to")
                  const patch = getStringValue(fileRecord, "patch")

                  return (
                    <details
                      key={`${filePath}-${index}`}
                      className="overflow-hidden rounded-md border border-border/30 bg-background/60"
                    >
                      <summary className="cursor-pointer px-2.5 py-2 text-[12px] text-foreground/80">
                        <span className="font-medium">{filePath}</span>
                        {kind ? (
                          <span className="ml-2 text-muted-foreground/60">{kind}</span>
                        ) : null}
                        {moveTo ? (
                          <span className="ml-2 text-muted-foreground/60">→ {moveTo}</span>
                        ) : null}
                      </summary>
                      <div className="space-y-2 border-t border-border/20 px-2.5 py-2">
                        <DetailList
                          entries={[
                            { label: "Added", value: getNumberValue(fileRecord, "added") },
                            {
                              label: "Removed",
                              value: getNumberValue(fileRecord, "removed"),
                            },
                          ]}
                        />
                        {patch ? (
                          <ToolDetailSection title="Patch">
                            <ExpandableOutput value={patch} failed={false} />
                          </ToolDetailSection>
                        ) : null}
                      </div>
                    </details>
                  )
                })}
              </div>
            </ToolDetailSection>
          ) : null}
        </div>
      )
    },
  }
}
