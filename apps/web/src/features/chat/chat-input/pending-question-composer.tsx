import { ChevronLeft, ChevronRight } from "lucide-react"
import { useEffect, useMemo, useState } from "react"

import { Textarea } from "@/components/ui/textarea"
import type { QuestionAnswer, QuestionItem, QuestionResult } from "@/lib/types"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"
import { usePendingQuestionStore } from "@/stores/pending-question-store"

function ChoiceQuestion({
  item,
  disabled,
  value,
  onChange,
}: {
  item: QuestionItem
  disabled: boolean
  value: QuestionAnswer | undefined
  onChange: (answer: QuestionAnswer) => void
}) {
  const selected = new Set(value?.selected_option_ids ?? [])
  const customText = value?.text ?? ""
  const [customInputFocused, setCustomInputFocused] = useState(false)
  const recommendedOptionIds = item.recommended_option_ids ?? []
  const recommendationReason = item.recommendation_reason ?? null
  const customInputActive = customInputFocused || customText.trim().length > 0

  return (
    <div className="space-y-2">
      {item.options.map((option) => {
        const checked = selected.has(option.id)
        const isRecommended = recommendedOptionIds.includes(option.id)
        return (
          <label
            key={option.id}
            className={cn(
              "flex cursor-pointer items-start gap-3 rounded-lg border border-border/50 px-3 py-2.5 transition-colors",
              isRecommended && !checked && "border-foreground/15 bg-muted/35",
              checked ? "border-foreground/20 bg-muted/60" : "bg-card hover:bg-muted/40",
              disabled && "cursor-not-allowed opacity-60"
            )}
          >
            <input
              type={item.multi_select ? "checkbox" : "radio"}
              name={item.id}
              checked={checked}
              disabled={disabled || customInputActive}
              onChange={() => {
                const next = item.multi_select
                  ? checked
                    ? [...selected].filter((optionId) => optionId !== option.id)
                    : [...selected, option.id]
                  : [option.id]
                onChange({
                  question_id: item.id,
                  selected_option_ids: next,
                  text: null,
                })
              }}
              className="mt-0.5"
            />
            <span className="min-w-0">
              <span className="flex items-center gap-2">
                <span className="text-ui block font-medium text-foreground">
                  {option.label}
                </span>
                {isRecommended ? (
                  <span className="text-caption rounded-full border border-foreground/10 bg-foreground/[0.06] px-1.5 py-0.5 text-foreground/75">
                    Recommended
                  </span>
                ) : null}
              </span>
              {option.description || (isRecommended && recommendationReason) ? (
                <span className="text-meta mt-0.5 block text-muted-foreground/70">
                  {option.description}
                  {option.description && isRecommended && recommendationReason
                    ? " — "
                    : ""}
                  {isRecommended && recommendationReason ? recommendationReason : ""}
                </span>
              ) : null}
            </span>
          </label>
        )
      })}

      <label
        className={cn(
          "flex items-start gap-3 rounded-lg border border-border/50 px-3 py-2.5 transition-colors",
          customInputActive ? "border-foreground/20 bg-muted/60" : "bg-card hover:bg-muted/40",
          disabled && "opacity-60"
        )}
      >
        <input
          type={item.multi_select ? "checkbox" : "radio"}
          name={item.id}
          checked={customInputActive}
          disabled={disabled}
          onChange={() => {
            setCustomInputFocused(true)
            onChange({
              question_id: item.id,
              selected_option_ids: [],
              text: customText,
            })
          }}
          className="mt-0.5"
        />
        <span className="min-w-0 flex-1">
          <span className="text-ui mb-1 block font-medium text-foreground">Custom input</span>
          <input
            type="text"
            value={customText}
            disabled={disabled}
            placeholder={item.placeholder ?? "Write your answer..."}
            onFocus={() => {
              setCustomInputFocused(true)
              onChange({
                question_id: item.id,
                selected_option_ids: [],
                text: customText,
              })
            }}
            onBlur={() => setCustomInputFocused(false)}
            onChange={(event) =>
              onChange({
                question_id: item.id,
                selected_option_ids: [],
                text: event.target.value,
              })
            }
            className="text-body-sm leading-body-sm h-6 w-full bg-transparent text-foreground outline-none placeholder:text-muted-foreground/35"
          />
        </span>
      </label>
    </div>
  )
}

function TextQuestion({
  item,
  disabled,
  value,
  onChange,
}: {
  item: QuestionItem
  disabled: boolean
  value: QuestionAnswer | undefined
  onChange: (answer: QuestionAnswer) => void
}) {
  return (
    <Textarea
      value={value?.text ?? ""}
      disabled={disabled}
      placeholder={item.placeholder ?? "Write your answer..."}
      onChange={(event) =>
        onChange({
          question_id: item.id,
          selected_option_ids: [],
          text: event.target.value,
        })
      }
      rows={3}
      className="min-h-[72px] resize-y rounded-lg border-border/40 bg-background/20 px-3 py-2.5 placeholder:text-muted-foreground/35"
    />
  )
}

function ConfirmQuestion(props: {
  item: QuestionItem
  disabled: boolean
  value: QuestionAnswer | undefined
  onChange: (answer: QuestionAnswer) => void
}) {
  const fallbackItem: QuestionItem = {
    ...props.item,
    kind: "choice",
    multi_select: false,
    options:
      props.item.options.length > 0
        ? props.item.options
        : [
            { id: "yes", label: "Yes", description: null },
            { id: "no", label: "No", description: null },
          ],
  }
  return <ChoiceQuestion {...props} item={fallbackItem} />
}

function questionValidationError(
  item: QuestionItem | null,
  answer: QuestionAnswer | undefined
): string | null {
  if (!item || !item.required) return null
  if (!answer) return `Please answer “${item.header}”.`
  const hasText = Boolean((answer.text ?? "").trim())
  if (item.kind === "text" && !hasText) {
    return `Please answer “${item.header}”.`
  }
  if (item.kind !== "text" && answer.selected_option_ids.length === 0 && !hasText) {
    return `Please answer “${item.header}”.`
  }
  return null
}

export function PendingQuestionComposer() {
  const activeSessionId = useChatStore((state) => state.activeSessionId)
  const pendingQuestion = usePendingQuestionStore((state) => state.pendingQuestion)
  const submitting = usePendingQuestionStore((state) => state.submitting)
  const storeError = usePendingQuestionStore((state) => state.error)
  const submitResult = usePendingQuestionStore((state) => state.submitResult)
  const cancel = usePendingQuestionStore((state) => state.cancel)

  const [answers, setAnswers] = useState<Record<string, QuestionAnswer>>({})
  const [questionIndex, setQuestionIndex] = useState(0)

  const questionItems = pendingQuestion?.questions ?? []
  const disabled = submitting || !activeSessionId || !pendingQuestion
  const currentItem = questionItems[questionIndex] ?? null
  const isLastQuestion = questionIndex >= questionItems.length - 1

  const validationError = useMemo(() => {
    if (!pendingQuestion) return null
    for (const item of pendingQuestion.questions) {
      if (!item.required) continue
      const answer = answers[item.id]
      if (!answer) return `Please answer “${item.header}”.`
      const hasText = Boolean((answer.text ?? "").trim())
      if (item.kind === "text" && !hasText) {
        return `Please answer “${item.header}”.`
      }
      if (item.kind !== "text" && answer.selected_option_ids.length === 0 && !hasText) {
        return `Please answer “${item.header}”.`
      }
    }
    return null
  }, [answers, pendingQuestion])

  const currentValidationError = useMemo(
    () => questionValidationError(currentItem, currentItem ? answers[currentItem.id] : undefined),
    [answers, currentItem]
  )

  useEffect(() => {
    setAnswers({})
    setQuestionIndex(0)
  }, [pendingQuestion?.request_id])

  if (!pendingQuestion) return null
  if (!currentItem) return null

  async function handleSubmit() {
    if (!activeSessionId || validationError) return
    const result: QuestionResult = {
      status: "answered",
      request_id: pendingQuestion.request_id,
      answers: questionItems.map((item) =>
        answers[item.id] ?? {
          question_id: item.id,
          selected_option_ids: [],
          text: null,
        }
      ),
    }
    await submitResult(activeSessionId, result)
    setAnswers({})
    setQuestionIndex(0)
  }

  async function handleDismiss() {
    if (!activeSessionId) return
    await cancel(activeSessionId)
    setAnswers({})
    setQuestionIndex(0)
  }

  async function handlePrimaryAction() {
    if (currentValidationError) return
    if (isLastQuestion) {
      await handleSubmit()
      return
    }
    setQuestionIndex((current) => Math.min(current + 1, questionItems.length - 1))
  }

  return (
    <div className="relative shrink-0 border-t border-border/30 px-4 pt-3 pb-4">
      <div className="pointer-events-none absolute -top-10 right-0 left-0 h-10 bg-gradient-to-t from-background to-transparent" />
      <div className="mx-auto w-full max-w-[720px] rounded-xl border border-border/50 bg-card px-4 py-3">
        <div className="mb-2 flex items-center justify-between gap-3">
          <div className="text-meta text-muted-foreground/50">
            {questionIndex + 1} / {questionItems.length}
          </div>
          <button
            type="button"
            disabled={disabled}
            onClick={() => void handleDismiss()}
            className="text-meta inline-flex h-7 shrink-0 items-center justify-center rounded-md px-1 text-muted-foreground/50 transition-colors hover:text-foreground/75 disabled:opacity-60"
            title="Dismiss question"
          >
            <span>Ignore</span>
          </button>
        </div>

        <section key={currentItem.id} className="space-y-2.5">
          <div className="space-y-0.5">
            <div className="text-ui font-medium text-foreground/95">{currentItem.header}</div>
            <p className="text-body-sm text-muted-foreground/70">{currentItem.question}</p>
          </div>

          {currentItem.kind === "choice" ? (
            <ChoiceQuestion
              item={currentItem}
              disabled={disabled}
              value={answers[currentItem.id]}
              onChange={(next) =>
                setAnswers((current) => ({ ...current, [currentItem.id]: next }))
              }
            />
          ) : null}

          {currentItem.kind === "text" ? (
            <TextQuestion
              item={currentItem}
              disabled={disabled}
              value={answers[currentItem.id]}
              onChange={(next) =>
                setAnswers((current) => ({ ...current, [currentItem.id]: next }))
              }
            />
          ) : null}

          {currentItem.kind === "confirm" ? (
            <ConfirmQuestion
              item={currentItem}
              disabled={disabled}
              value={answers[currentItem.id]}
              onChange={(next) =>
                setAnswers((current) => ({ ...current, [currentItem.id]: next }))
              }
            />
          ) : null}
        </section>

        {storeError && (
          <div className="text-meta mt-3 text-destructive/80">
            {storeError}
          </div>
        )}

        <div className="mt-3 flex items-center justify-end gap-2">
          {questionIndex > 0 ? (
            <button
              type="button"
              disabled={disabled}
              onClick={() => setQuestionIndex((current) => Math.max(current - 1, 0))}
              className="text-ui inline-flex h-7 items-center gap-1 rounded-lg border border-border/50 px-2.5 text-muted-foreground transition-colors hover:bg-muted/50 disabled:opacity-40"
            >
              <ChevronLeft className="size-4" />
              <span>Back</span>
            </button>
          ) : null}
          <button
            type="button"
            disabled={disabled || currentValidationError != null}
            onClick={() => void handlePrimaryAction()}
            className="text-ui inline-flex h-7 items-center gap-1 rounded-lg bg-foreground px-2.5 text-background transition-opacity hover:opacity-85 disabled:opacity-50"
          >
            <span>{submitting ? "Submitting..." : isLastQuestion ? "Submit" : "Next"}</span>
            {!submitting && !isLastQuestion ? <ChevronRight className="size-4" /> : null}
            </button>
        </div>
      </div>
    </div>
  )
}
