import type { ToolOutputSegment } from "@/lib/types"
import type { ReactNode } from "react"

export type ToolRenderData = {
  invocationId?: string
  turnId?: string
  toolName: string
  arguments: Record<string, unknown>
  rawArguments?: string
  previewHtml?: string
  details?: Record<string, unknown>
  outputContent: string
  outputSegments?: ToolOutputSegment[]
  succeeded: boolean
  isRunning?: boolean
  detectedAtMs?: number
  startedAtMs?: number
  finishedAtMs?: number
}

export type ToolRenderer = {
  matches: (toolName: string) => boolean
  detailsPanelMode?: "default" | "renderer-only" | "renderer-only-flat" | "none"
  /** Whether to show duration before tool has started (default: true) */
  showDurationBeforeStart?: boolean
  renderTitle: (data: ToolRenderData) => string
  renderSubtitle?: (data: ToolRenderData) => string | null
  /** Render the trigger subtitle with full control (e.g., file path with streaming shimmer) */
  renderTriggerSubtitle?: (data: ToolRenderData) => ReactNode | null
  renderMeta?: (data: ToolRenderData) => ReactNode | null
  renderDetails: (data: ToolRenderData) => ReactNode | null
}

export type ToolRendererRegistry = {
  register: (renderer: ToolRenderer) => void
  resolve: (toolName: string) => ToolRenderer
  renderTitle: (data: ToolRenderData) => string
  renderSubtitle: (data: ToolRenderData) => string | null
  renderTriggerSubtitle: (data: ToolRenderData) => ReactNode | null
  renderMeta: (data: ToolRenderData) => ReactNode | null
  renderDetails: (data: ToolRenderData) => ReactNode | null
  shouldShowDurationBeforeStart: (toolName: string) => boolean
}
