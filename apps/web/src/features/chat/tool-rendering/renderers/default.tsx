import type { ToolRenderer } from "../types"
import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"
import { toolTimelineCopy } from "../../tool-timeline-copy"
import { buildDetailEntries, truncateInline } from "../helpers"
import {
  DetailList,
  ExpandableOutput,
  ToolDetailSection,
  ToolInfoSection,
} from "../ui"

function buildFallbackTitle(
  data: Parameters<ToolRenderer["renderTitle"]>[0]
): string {
  const primary = getToolDisplayPath(
    data.toolName,
    data.details,
    data.arguments
  )

  return primary
    ? `Unknown · ${data.toolName} — ${truncateInline(primary, 64)}`
    : `Unknown · ${data.toolName}`
}

export function createDefaultToolRenderer(): ToolRenderer {
  return {
    matches: () => true,
    renderTitle(data) {
      return buildFallbackTitle(data)
    },
    renderMeta() {
      return null
    },
    renderDetails(data) {
      const normalizedArgs = normalizeToolArguments(data.arguments)
      const argumentEntries = buildDetailEntries(normalizedArgs)
      const detailEntries = buildDetailEntries(data.details)
      const hasResult = Boolean(data.outputContent)

      if (
        argumentEntries.length === 0 &&
        detailEntries.length === 0 &&
        !hasResult
      ) {
        return null
      }

      return (
        <div className="space-y-3">
          {argumentEntries.length > 0 ? (
            <ToolInfoSection title={toolTimelineCopy.section.request}>
              <DetailList entries={argumentEntries} />
            </ToolInfoSection>
          ) : null}
          {detailEntries.length > 0 ? (
            <ToolInfoSection title={toolTimelineCopy.section.rawDetails}>
              <DetailList entries={detailEntries} />
            </ToolInfoSection>
          ) : null}
          {hasResult ? (
            <ToolDetailSection
              title={
                data.succeeded
                  ? toolTimelineCopy.section.result
                  : toolTimelineCopy.section.failure
              }
            >
              <ExpandableOutput
                value={data.outputContent}
                failed={!data.succeeded}
              />
            </ToolDetailSection>
          ) : null}
        </div>
      )
    },
  }
}
