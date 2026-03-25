import type { ToolRenderer } from "../types"
import {
  getToolDisplayName,
  getToolDisplayPath,
  normalizeToolArguments,
} from "@/lib/tool-display"
import { toolTimelineCopy } from "../../tool-timeline-copy"
import { buildDetailEntries, truncateInline } from "../helpers"
import {
  DetailList,
  ExpandableOutput,
  ToolDetailSection,
  ToolInfoSection,
} from "../ui"

const PRIMARY_TITLE_KEYS = new Set([
  "path",
  "file_path",
  "pattern",
  "command",
  "cmd",
  "query",
])

function buildFallbackTitle(
  data: Parameters<ToolRenderer["renderTitle"]>[0]
): string {
  const normalizedArgs = normalizeToolArguments(data.arguments)
  const primary = getToolDisplayPath(
    data.toolName,
    data.details,
    normalizedArgs
  )
  const titleSegments = [
    primary ? truncateInline(primary, 64) : null,
    ...buildDetailEntries(normalizedArgs, {
      omitKeys: PRIMARY_TITLE_KEYS,
      maxEntries: primary ? 1 : 2,
    }).map((entry) => truncateInline(`${entry.label} ${entry.value}`, 40)),
    ...buildDetailEntries(data.details, {
      omitKeys: PRIMARY_TITLE_KEYS,
      maxEntries: primary ? 1 : 2,
    }).map((entry) => truncateInline(`${entry.label} ${entry.value}`, 40)),
  ].filter((segment): segment is string => Boolean(segment))
  const summary = titleSegments.slice(0, primary ? 3 : 2).join(" · ")

  return summary || getToolDisplayName(data.toolName)
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
