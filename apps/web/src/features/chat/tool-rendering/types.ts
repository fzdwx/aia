import type { ToolOutputSegment } from "@/lib/types"
import type { ReactNode } from "react"

export type ToolRenderData = {
  toolName: string
  arguments: Record<string, unknown>
  details?: Record<string, unknown>
  outputContent: string
  outputSegments?: ToolOutputSegment[]
  succeeded: boolean
  isRunning?: boolean
}

export type ToolRenderer = {
  matches: (toolName: string) => boolean
  renderTitle: (data: ToolRenderData) => string
  renderSubtitle?: (data: ToolRenderData) => string | null
  renderMeta?: (data: ToolRenderData) => ReactNode | null
  renderDetails: (data: ToolRenderData) => ReactNode | null
}

export type ToolRendererRegistry = {
  register: (renderer: ToolRenderer) => void
  resolve: (toolName: string) => ToolRenderer
  renderTitle: (data: ToolRenderData) => string
  renderSubtitle: (data: ToolRenderData) => string | null
  renderMeta: (data: ToolRenderData) => ReactNode | null
  renderDetails: (data: ToolRenderData) => ReactNode | null
}
