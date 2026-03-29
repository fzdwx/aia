import type {
  ToolRenderData,
  ToolRenderer,
  ToolRendererRegistry,
} from "./types"

function getToolNameSegment(toolName: string): string {
  const trimmed = toolName.trim()
  if (!trimmed) return toolName
  const segments = trimmed.split(".")
  return segments[segments.length - 1] ?? trimmed
}
export function createToolRendererRegistry(
  defaultRenderer: ToolRenderer,
  initialRenderers: ToolRenderer[] = []
): ToolRendererRegistry {
  const renderers = [...initialRenderers]

  return {
    register(renderer) {
      renderers.unshift(renderer)
    },
    resolve(toolName) {
      const normalizedToolName = getToolNameSegment(toolName)
      return (
        renderers.find((renderer) => renderer.matches(normalizedToolName)) ??
        defaultRenderer
      )
    },
    renderTitle(data: ToolRenderData) {
      return this.resolve(data.toolName).renderTitle(data)
    },
    renderSubtitle(data: ToolRenderData) {
      return this.resolve(data.toolName).renderSubtitle?.(data) ?? null
    },
    renderTriggerSubtitle(data: ToolRenderData) {
      return this.resolve(data.toolName).renderTriggerSubtitle?.(data) ?? null
    },
    renderMeta(data: ToolRenderData) {
      return this.resolve(data.toolName).renderMeta?.(data) ?? null
    },
    renderDetails(data: ToolRenderData) {
      return this.resolve(data.toolName).renderDetails(data)
    },
  }
}
