import { useState } from "react"

import { DeferredServerPatchDiffOutput } from "../../diff/server-diff"

export type ApplyPatchOperation = {
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

export function DeferredPatchOperation({
  entry,
}: {
  entry: ApplyPatchOperation
}) {
  const [isOpen, setIsOpen] = useState(false)

  return (
    <details
      className="tool-timeline-patch-item"
      data-kind={entry.kind}
      onToggle={(e) => {
        const target = e.target as HTMLDetailsElement
        setIsOpen(target.open)
      }}
    >
      <summary className="tool-timeline-patch-summary">
        <span className="tool-timeline-patch-path" title={entry.displayPath}>
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
        {isOpen && <DeferredServerPatchDiffOutput patch={entry.patch} />}
      </div>
    </details>
  )
}
