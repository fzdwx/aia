import { WorkerPoolContextProvider } from "@pierre/diffs/react"
import type { ReactNode } from "react"

import { pierreWorkerPoolOptions } from "./pierre-worker"

const PIERRE_WORKER_HIGHLIGHT_OPTIONS = {
  preferredHighlighter: "shiki-js",
  tokenizeMaxLineLength: 1000,
  lineDiffType: "none",
} as const

export function PierreDiffProvider({ children }: { children: ReactNode }) {
  return (
    <WorkerPoolContextProvider
      poolOptions={pierreWorkerPoolOptions}
      highlighterOptions={PIERRE_WORKER_HIGHLIGHT_OPTIONS}
    >
      {children}
    </WorkerPoolContextProvider>
  )
}
