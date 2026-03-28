import { normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import {
  getArrayValue,
  getBooleanValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import { ExpandableOutput, ToolDetailSection, ToolDetailSurface } from "../ui"

type QuestionOption = {
  id: string
  label: string
  description?: string | null
}

type QuestionItem = {
  id: string
  question: string
  kind: string
  required?: boolean
  multi_select?: boolean
  options?: QuestionOption[]
  recommended_option_id?: string | null
  recommendation_reason?: string | null
}

type QuestionAnswer = {
  question_id: string
  selected_option_ids?: string[]
  text?: string | null
}

type QuestionResult = {
  status?: string
  answers?: QuestionAnswer[]
  reason?: string | null
}

function extractRawQuestionPrompts(record: Record<string, unknown>): string[] {
  return getArrayValue(record, "questions").flatMap((item) => {
    if (!item || typeof item !== "object") return []
    const value = item as Record<string, unknown>
    const question = getStringValue(
      value,
      "question",
      "label",
      "prompt",
      "message"
    )
    return question ? [question] : []
  })
}

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

function parseQuestionItems(record: Record<string, unknown>): QuestionItem[] {
  return getArrayValue(record, "questions").flatMap((item) => {
    if (!item || typeof item !== "object") return []
    const value = item as Record<string, unknown>
    const id = getStringValue(value, "id")
    const question = getStringValue(value, "question")
    const kind = getStringValue(value, "kind")
    if (!id || !question || !kind) return []

    return [
      {
        id,
        question,
        kind,
        required: getBooleanValue(value, "required"),
        multi_select: getBooleanValue(value, "multi_select"),
        recommended_option_id:
          getStringValue(value, "recommended_option_id") ?? null,
        recommendation_reason:
          getStringValue(value, "recommendation_reason") ?? null,
        options: getArrayValue(value, "options").flatMap((option) => {
          if (!option || typeof option !== "object") return []
          const next = option as Record<string, unknown>
          const optionId = getStringValue(next, "id")
          const label = getStringValue(next, "label")
          if (!optionId || !label) return []
          return [
            {
              id: optionId,
              label,
              description: getStringValue(next, "description") ?? null,
            },
          ]
        }),
      },
    ]
  })
}

function parseQuestionResult(
  details?: Record<string, unknown>
): QuestionResult | null {
  if (!details) return null

  return {
    status: getStringValue(details, "status") ?? undefined,
    reason: getStringValue(details, "reason") ?? null,
    answers: getArrayValue(details, "answers").flatMap((answer) => {
      if (!answer || typeof answer !== "object") return []
      const value = answer as Record<string, unknown>
      const questionId = getStringValue(value, "question_id")
      if (!questionId) return []
      return [
        {
          question_id: questionId,
          selected_option_ids: getArrayValue(
            value,
            "selected_option_ids"
          ).flatMap((selected) =>
            typeof selected === "string" ? [selected] : []
          ),
          text: getStringValue(value, "text") ?? null,
        },
      ]
    }),
  }
}

function statusLabel(status: string | undefined): string {
  switch (status) {
    case "answered":
      return "answered"
    case "cancelled":
      return "cancelled"
    case "dismissed":
      return "dismissed"
    case "timed_out":
      return "timed out"
    case "unavailable":
      return "unavailable"
    default:
      return "waiting"
  }
}

function getQuestionSummary(data: {
  arguments: Record<string, unknown>
  details?: Record<string, unknown>
}): string {
  const questions = parseQuestionItems(data.arguments)
  const firstQuestion =
    questions[0]?.question ?? extractRawQuestionPrompts(data.arguments)[0]
  const summary =
    firstQuestion ??
    getStringValue(
      data.arguments,
      "question",
      "prompt",
      "summary",
      "message",
      "query"
    ) ??
    getStringValue(data.details, "question", "summary", "message")

  return summary ? truncateInline(summary, 96) : "Awaiting structured question"
}

function resolveAnswerSummary(
  item: QuestionItem,
  answer: QuestionAnswer | undefined
): string {
  if (!answer) return "No answer"
  const labels = (answer.selected_option_ids ?? [])
    .map(
      (selectedId) =>
        item.options?.find((option) => option.id === selectedId)?.label ??
        selectedId
    )
    .filter(Boolean)
  const customText = answer.text?.trim()

  if (labels.length > 0 && customText) {
    return `${labels.join(", ")} · ${truncateInline(customText, 96)}`
  }
  if (labels.length > 0) return labels.join(", ")
  if (customText) return truncateInline(customText, 96)
  return "No answer"
}

function buildQuestionSubtitle(data: {
  arguments: Record<string, unknown>
  details?: Record<string, unknown>
}): string | null {
  const questions = parseQuestionItems(data.arguments)
  const result = parseQuestionResult(data.details)
  if (questions.length === 0) return null

  const answeredCount = (result?.answers ?? []).length
  if (result?.status === "answered") {
    return `${answeredCount} answered`
  }

  if (result?.status && result.status !== "answered") {
    return statusLabel(result.status)
  }

  return null
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
    matches: (toolName) => toolName === "Question" || toolName === "question",
    detailsPanelMode: "renderer-only",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const questions = parseQuestionItems(args)
      if (questions.length > 1) {
        return "Questions"
      }
      return getQuestionSummary({ arguments: args, details: data.details })
    },
    renderSubtitle(data) {
      const args = normalizeToolArguments(data.arguments)
      return buildQuestionSubtitle({ arguments: args, details: data.details })
    },
    renderMeta(data) {
      const args = normalizeToolArguments(data.arguments)
      const questions = parseQuestionItems(args)
      const result = parseQuestionResult(data.details)

      if (questions.length > 0) return null

      return (
        <div className="flex flex-wrap items-center gap-2">
          <span className="tool-timeline-meta-badge">
            {statusLabel(result?.status)}
          </span>
          {questions.length > 1 ? (
            <span className="tool-timeline-meta-badge">
              {questions.length} questions
            </span>
          ) : null}
        </div>
      )
    },
    renderDetails(data) {
      const args = normalizeToolArguments(data.arguments)
      const result = parseQuestionResult(data.details)
      const questions = parseQuestionItems(args)
      const ignored = isIgnored({
        arguments: args,
        details: data.details,
        outputContent: data.outputContent,
      })

      if (questions.length === 0) {
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
      }

      const answersByQuestionId = new Map(
        (result?.answers ?? []).map((answer) => [answer.question_id, answer])
      )

      return (
        <div className="space-y-3">
          <div className="space-y-2">
            {questions.map((item) => {
              const answer = answersByQuestionId.get(item.id)
              const answerSummary = resolveAnswerSummary(item, answer)
              const hasAnswer = answerSummary !== "No answer"
              return (
                <ToolDetailSurface
                  key={item.id}
                  className="space-y-2 border-none bg-transparent px-0 py-0 shadow-none"
                >
                  <div className="space-y-1">
                    <p className="text-body-sm text-muted-foreground/78">
                      {item.question}
                    </p>
                    <div
                      className={
                        hasAnswer
                          ? "text-body-sm leading-6 font-medium text-foreground"
                          : "text-body-sm text-muted-foreground/75 italic"
                      }
                    >
                      {hasAnswer ? answerSummary : "(no answer)"}
                    </div>
                    {item.recommendation_reason && !hasAnswer ? (
                      <p className="text-meta text-muted-foreground/65">
                        Recommended: {item.recommendation_reason}
                      </p>
                    ) : null}
                  </div>
                </ToolDetailSurface>
              )
            })}
          </div>

          {result?.reason ? (
            <ToolDetailSection title="Result">
              <ExpandableOutput value={result.reason} failed={false} />
            </ToolDetailSection>
          ) : null}
        </div>
      )
    },
  }
}
