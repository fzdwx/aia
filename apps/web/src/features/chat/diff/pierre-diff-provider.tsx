import { WorkerPoolContext } from "@pierre/diffs/react"
import { useState, type ReactNode } from "react"

import { getPierreWorkerPool } from "./pierre-worker"

export function PierreDiffProvider({ children }: { children: ReactNode }) {
  const [workerPool] = useState(() => getPierreWorkerPool())

  return (
    <WorkerPoolContext.Provider value={workerPool}>
      {children}
    </WorkerPoolContext.Provider>
  )
}
