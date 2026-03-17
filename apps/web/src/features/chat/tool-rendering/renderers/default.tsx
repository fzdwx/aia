import type { ToolRenderer } from "../types"
import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"
import { formatScalar, truncateInline } from "../helpers"
import { ExpandableOutput, ToolDetailSection } from "../ui"

export function createDefaultToolRenderer(): ToolRenderer {
  return {
    matches: () => true,
    renderTitle(data) {
      const primary = getToolDisplayPath(
        data.toolName,
        data.details,
        data.arguments
      )
      const entries = Object.entries(normalizeToolArguments(data.arguments))
        .filter(
          ([, value]) =>
            value == null ||
            typeof value === "string" ||
            typeof value === "number" ||
            typeof value === "boolean"
        )
        .slice(0, 3)
        .map(
          ([key, value]) => `${key}: ${truncateInline(formatScalar(value), 32)}`
        )

      return [primary, entries.join(" · ")].filter(Boolean).join(" — ")
    },
    renderMeta() {
      return null
    },
    renderDetails(data) {
      if (!data.outputContent) return null

      return (
        <ToolDetailSection title={data.succeeded ? "Content" : "Failure"}>
          <ExpandableOutput
            value={data.outputContent}
            failed={!data.succeeded}
          />
        </ToolDetailSection>
      )
    },
  }
}
