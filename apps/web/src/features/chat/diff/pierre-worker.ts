import {
  WorkerPoolManager,
  type WorkerInitializationRenderOptions,
  type WorkerPoolOptions,
} from "@pierre/diffs/worker"
import WorkerUrl from "@pierre/diffs/worker/worker.js?worker&url"

export function createPierreDiffWorker(): Worker {
  return new Worker(WorkerUrl, { type: "module" })
}

// The library defaults to 8 workers and a large AST cache, which is excessive
// for our chat timeline where many historical diffs can be mounted at once.
export const pierreWorkerPoolOptions: WorkerPoolOptions = {
  workerFactory: createPierreDiffWorker,
  poolSize: 2,
  totalASTLRUCacheSize: 24,
}

export const pierreWorkerHighlighterOptions: WorkerInitializationRenderOptions =
  {
    preferredHighlighter: "shiki-js",
    tokenizeMaxLineLength: 1000,
    lineDiffType: "none",
  }

type PierreWorkerPoolGlobal = typeof globalThis & {
  __AIA_PIERRE_WORKER_POOL__?: WorkerPoolManager
}

function createPierreWorkerPool() {
  const pool = new WorkerPoolManager(
    pierreWorkerPoolOptions,
    pierreWorkerHighlighterOptions
  )
  pool.initialize()
  return pool
}

export function getPierreWorkerPool(): WorkerPoolManager | undefined {
  if (typeof window === "undefined") {
    return undefined
  }

  const globalScope = globalThis as PierreWorkerPoolGlobal
  globalScope.__AIA_PIERRE_WORKER_POOL__ ??= createPierreWorkerPool()
  return globalScope.__AIA_PIERRE_WORKER_POOL__
}
