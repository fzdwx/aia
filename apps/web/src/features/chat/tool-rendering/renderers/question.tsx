import { normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import { getBooleanValue, getStringValue, truncateInline } from "../helpers"
import { ExpandableOutput, ToolDetailSection } from "../ui"

function isIgnored(data: {
  arguments: Record<string, unknown>
  details?: Record<string, unknown>
  outputContent: string
}): boolean {
  const status = getStringValue(data.details, "status", "state")?.toLowerCase()
  const action = getStringValue(data.details, "action")?.toLowerCase()

  return (
    getBooleanValue(data.details, "ignored", "was_ignored", "skip_render") ===
      true ||
    getBooleanValue(data.arguments, "ignored") === true ||
    status === "ignored" ||
    action === "ignore" ||
    action === "ignored" ||
    data.outputContent.includes("ignored")
  )
}

function getQuestionSummary(data: {
  arguments: Record<string, unknown>
  details?: Record<string, unknown>
}): string {
  const summary =
    getStringValue(
      data.arguments,
      "question",
      "prompt",
      "summary",
      "message",
      "query"
    ) ?? getStringValue(data.details, "question", "summary", "message")

  return summary ? truncateInline(summary, 96) : toolTimelineCopy.section.issue
}

function getQuestionContent(data: {
  arguments: Record<string, unknown>
  details?: Record<string, unknown>
  outputContent: string
}): string {
  const content =
    getStringValue(data.details, "summary", "answer", "reason", "message") ??
    data.outputContent

  if (content) return content
  if (isIgnored(data)) return "Question ignored."
  return "No additional details."
}

export function createQuestionRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "question",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      return getQuestionSummary({ arguments: args, details: data.details })
    },
    renderDetails(data) {
      const args = normalizeToolArguments(data.arguments)
      const ignored = isIgnored({
        arguments: args,
        details: data.details,
        outputContent: data.outputContent,
      })

      return (
        <ToolDetailSection
          title={
            ignored
              ? toolTimelineCopy.section.issueIgnored
              : toolTimelineCopy.section.issue
          }
        >
          <ExpandableOutput
            value={getQuestionContent({
              arguments: args,
              details: data.details,
              outputContent: data.outputContent,
            })}
            failed={false}
          />
        </ToolDetailSection>
      )
    },
  }
}
