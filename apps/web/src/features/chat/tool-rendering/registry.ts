import type {
  ToolRenderData,
  ToolRenderer,
  ToolRendererRegistry,
} from "./types"

function normalizeToolName(toolName: string): string {
  const lower = toolName.toLowerCase()
  const segments = lower.split(".")
  return segments[segments.length - 1] ?? lower
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
      const normalizedToolName = normalizeToolName(toolName)
      return (
        renderers.find((renderer) => renderer.matches(normalizedToolName)) ??
        defaultRenderer
      )
    },
    renderTitle(data: ToolRenderData) {
      return this.resolve(data.toolName).renderTitle(data)
    },
    renderDetails(data: ToolRenderData) {
      return this.resolve(data.toolName).renderDetails(data)
    },
  }
}
