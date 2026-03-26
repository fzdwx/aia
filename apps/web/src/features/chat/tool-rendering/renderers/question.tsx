import { normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import {
  getArrayValue,
  getBooleanValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import {
  DetailList,
  ExpandableOutput,
  ToolDetailSection,
  ToolDetailSurface,
} from "../ui"

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
      return "Answered"
    case "cancelled":
      return "Cancelled"
    case "dismissed":
      return "Dismissed"
    case "timed_out":
      return "Timed Out"
    case "unavailable":
      return "Unavailable"
    default:
      return "Question"
  }
}

function kindLabel(kind: string): string {
  switch (kind) {
    case "choice":
      return "Choice"
    case "text":
      return "Text"
    case "confirm":
      return "Confirm"
    default:
      return kind
  }
}

function getQuestionSummary(data: {
  arguments: Record<string, unknown>
  details?: Record<string, unknown>
}): string {
  const questions = parseQuestionItems(data.arguments)
  const firstQuestion = questions[0]?.question
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

  return summary ? truncateInline(summary, 96) : toolTimelineCopy.section.issue
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
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      return getQuestionSummary({ arguments: args, details: data.details })
    },
    renderMeta(data) {
      const args = normalizeToolArguments(data.arguments)
      const questions = parseQuestionItems(args)
      const result = parseQuestionResult(data.details)

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
          <ToolDetailSection title="Question Summary">
            <ToolDetailSurface>
              <DetailList
                entries={[
                  { label: "Status", value: statusLabel(result?.status) },
                  { label: "Questions", value: questions.length },
                  {
                    label: "Answered",
                    value: (result?.answers ?? []).length,
                  },
                ]}
              />
              {result?.reason ? (
                <ExpandableOutput value={result.reason} failed={false} />
              ) : null}
            </ToolDetailSurface>
          </ToolDetailSection>

          <ToolDetailSection title="Question Flow">
            <div className="space-y-2">
              {questions.map((item, index) => {
                const answer = answersByQuestionId.get(item.id)
                return (
                  <ToolDetailSurface key={item.id} className="space-y-2">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="tool-timeline-meta-badge">
                        {index + 1}
                      </span>
                      <span className="tool-timeline-meta-badge">
                        {kindLabel(item.kind)}
                      </span>
                      {item.required ? (
                        <span className="tool-timeline-meta-badge">
                          Required
                        </span>
                      ) : null}
                    </div>

                    <div className="space-y-1">
                      <p className="text-ui font-medium text-foreground">
                        {item.question}
                      </p>
                      {item.recommendation_reason ? (
                        <p className="text-meta text-muted-foreground/75">
                          Recommended: {item.recommendation_reason}
                        </p>
                      ) : null}
                    </div>

                    {item.options && item.options.length > 0 ? (
                      <DetailList
                        entries={item.options.map((option) => ({
                          label: option.label,
                          value:
                            option.id === item.recommended_option_id
                              ? option.description
                                ? `${option.description} · recommended`
                                : "recommended"
                              : (option.description ?? "-"),
                        }))}
                      />
                    ) : null}

                    <div className="space-y-1">
                      <p className="text-meta font-medium text-muted-foreground/75">
                        Answer
                      </p>
                      <ExpandableOutput
                        value={resolveAnswerSummary(item, answer)}
                        failed={false}
                      />
                    </div>
                  </ToolDetailSurface>
                )
              })}
            </div>
          </ToolDetailSection>
        </div>
      )
    },
  }
}
