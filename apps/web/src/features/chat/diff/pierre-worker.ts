import type { WorkerPoolOptions } from "@pierre/diffs/worker"
import WorkerUrl from "@pierre/diffs/worker/worker.js?worker&url"

export function createPierreDiffWorker(): Worker {
  return new Worker(WorkerUrl, { type: "module" })
}

export const pierreWorkerPoolOptions: WorkerPoolOptions = {
  workerFactory: createPierreDiffWorker,
}
