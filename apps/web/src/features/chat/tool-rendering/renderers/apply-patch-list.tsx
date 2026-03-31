import { useMemo, useState } from "react"

import { LazyDiffMount } from "../../diff/lazy-diff-mount"
import { PierrePatchDiffOutput } from "../../diff/pierre-diff"

import { toPatchOperations, type ApplyPatchOperation } from "./apply-patch"

function ApplyPatchOperationItem({ entry }: { entry: ApplyPatchOperation }) {
  const [isOpen, setIsOpen] = useState(false)

  return (
    <details
      className="tool-timeline-patch-item"
      data-kind={entry.kind}
      onToggle={(event) => {
        setIsOpen(event.currentTarget.open)
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
      {isOpen ? (
        <div className="tool-timeline-patch-body">
          <LazyDiffMount>
            <PierrePatchDiffOutput patch={entry.patch} />
          </LazyDiffMount>
        </div>
      ) : null}
    </details>
  )
}

export function ApplyPatchOperationList({ patch }: { patch: string }) {
  const operations = useMemo(() => toPatchOperations(patch), [patch])

  if (operations.length === 0) return null

  return (
    <div className="tool-timeline-patch-list">
      {operations.map((entry) => (
        <ApplyPatchOperationItem key={entry.key} entry={entry} />
      ))}
    </div>
  )
}
