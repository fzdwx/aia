import { toolRendererRegistry } from "@/features/chat/tool-rendering"
import { buildDetailEntries } from "@/features/chat/tool-rendering/helpers"
import {
  DetailList,
  ToolDetailSurface,
  ToolInfoSection,
} from "@/features/chat/tool-rendering/ui"
import type { ToolRowItem } from "@/features/chat/tool-timeline-helpers"
import { normalizeToolName } from "@/features/chat/tool-timeline-helpers"

import { toolTimelineCopy } from "@/features/chat/tool-timeline-copy"

const NON_DEFAULT_TOOL_NAMES = new Set([
  "Read",
  "CodeSearch",
  "WebSearch",
  "Glob",
  "Grep",
  "WidgetReadme",
  "WidgetRenderer",
])
const OMITTED_ARGUMENT_KEYS = new Set([
  "content",
  "patch",
  "patchText",
  "old_string",
  "new_string",
  "value",
  "text",
  "input",
  "contents",
])
const OMITTED_DETAIL_KEYS = new Set([
  "stdout",
  "stderr",
  "diff",
  "content",
  "file_path",
  "path",
  "pattern",
  "command",
])

export function renderToolDetailsPanel(item: ToolRowItem) {
  const normalizedToolName = normalizeToolName(item.toolName)
  const renderData = {
    toolName: item.toolName,
    arguments: item.arguments,
    details: item.details,
    outputContent: item.outputContent,
    outputSegments: item.outputSegments,
    succeeded: item.succeeded,
    isRunning: item.finishedAtMs == null,
  }
  const renderer = toolRendererRegistry.resolve(item.toolName)
  const detailsContent = toolRendererRegistry.renderDetails(renderData)
  const detailsPanelMode = renderer.detailsPanelMode ?? "default"

  if (detailsPanelMode === "none") {
    return null
  }

  if (detailsPanelMode === "renderer-only") {
    if (detailsContent == null) return null

    return <ToolDetailSurface>{detailsContent}</ToolDetailSurface>
  }

  if (detailsPanelMode === "renderer-only-flat") {
    if (detailsContent == null) return null

    if (normalizedToolName === "ApplyPatch") {
      return (
        <ToolDetailSurface className="tool-timeline-detail-surface-flat tool-timeline-detail-surface-borderless">
          {detailsContent}
        </ToolDetailSurface>
      )
    }

    return (
      <ToolDetailSurface className="tool-timeline-detail-surface-flat">
        {detailsContent}
      </ToolDetailSurface>
    )
  }

  const requestEntries = buildDetailEntries(item.arguments, {
    omitKeys: OMITTED_ARGUMENT_KEYS,
  })
  const resultEntries = buildDetailEntries(item.details, {
    omitKeys: OMITTED_DETAIL_KEYS,
  })
  const usesDefaultRenderer = !NON_DEFAULT_TOOL_NAMES.has(normalizedToolName)

  if (usesDefaultRenderer && detailsContent != null) {
    return <ToolDetailSurface>{detailsContent}</ToolDetailSurface>
  }

  if (
    requestEntries.length === 0 &&
    resultEntries.length === 0 &&
    detailsContent == null
  ) {
    return null
  }

  return (
    <ToolDetailSurface>
      {requestEntries.length > 0 ? (
        <ToolInfoSection
          title={toolTimelineCopy.section.request}
          hint={`${requestEntries.length} ${toolTimelineCopy.unit.field}`}
        >
          <DetailList entries={requestEntries} />
        </ToolInfoSection>
      ) : null}
      {resultEntries.length > 0 ? (
        <ToolInfoSection
          title={toolTimelineCopy.section.result}
          hint={`${resultEntries.length} ${toolTimelineCopy.unit.field}`}
          defaultOpen={false}
        >
          <DetailList entries={resultEntries} />
        </ToolInfoSection>
      ) : null}
      {detailsContent ? detailsContent : null}
    </ToolDetailSurface>
  )
}
