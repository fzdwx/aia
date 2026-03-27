import { Check } from "lucide-react"
import { useEffect, useMemo, useRef, useState } from "react"

import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import type { QuestionAnswer, QuestionItem, QuestionResult } from "@/lib/types"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"
import { usePendingQuestionStore } from "@/stores/pending-question-store"

function ChoiceIndicator({
  checked,
  multiSelect,
}: {
  checked: boolean
  multiSelect: boolean
}) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "mt-0.5 inline-flex size-4 shrink-0 items-center justify-center border border-border/70 bg-background",
        multiSelect ? "rounded-[4px]" : "rounded-full",
        checked &&
          (multiSelect
            ? "border-foreground/35 bg-foreground text-background"
            : "border-foreground/35 bg-foreground")
      )}
    >
      {multiSelect ? (
        <Check
          className={cn("size-3", checked ? "opacity-100" : "opacity-0")}
        />
      ) : (
        <span
          className={cn(
            "size-1.5 rounded-full bg-background",
            checked ? "opacity-100" : "opacity-0"
          )}
        />
      )}
    </span>
  )
}

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
  const customTextareaRef = useRef<HTMLTextAreaElement | null>(null)
  const recommendedOptionId = item.recommended_option_id ?? null
  const recommendationReason = item.recommendation_reason ?? null
  const customInputActive = customInputFocused || customText.trim().length > 0

  function activateCustomInput() {
    setCustomInputFocused(true)
    onChange({
      question_id: item.id,
      selected_option_ids: [],
      text: customText,
    })
  }

  useEffect(() => {
    if (!customInputFocused) return
    const element = customTextareaRef.current
    if (!element) return
    element.focus()
    element.style.height = "0px"
    element.style.height = `${element.scrollHeight}px`
  }, [customInputFocused])

  return (
    <div className="space-y-1.5">
      {item.options.map((option) => {
        const checked = selected.has(option.id)
        const isRecommended = recommendedOptionId === option.id
        const quietRecommendation = isRecommended
          ? recommendationReason
            ? `Recommended: ${recommendationReason}`
            : "Recommended"
          : null
        const hint = [option.description, quietRecommendation]
          .filter((text): text is string => Boolean(text))
          .join(" · ")

        return (
          <label
            key={option.id}
            className={cn(
              "flex cursor-pointer items-start gap-3 rounded-md border px-2.5 py-2 text-left focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50",
              isRecommended && !checked && "bg-muted/35",
              checked
                ? "border-transparent bg-muted/65 shadow-sm"
                : "border-border/55 bg-background/35 hover:bg-muted/35",
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
              className="sr-only"
            />
            <ChoiceIndicator
              checked={checked}
              multiSelect={item.multi_select}
            />
            <span className="min-w-0 flex-1 space-y-0.5">
              <span className="text-ui block font-medium text-pretty text-foreground">
                {option.label}
              </span>
              {hint ? (
                <span className="text-ui block text-pretty text-muted-foreground/80">
                  {hint}
                </span>
              ) : null}
            </span>
          </label>
        )
      })}

      <label
        className={cn(
          "flex items-start gap-3 rounded-md border px-2.5 py-2 text-left focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50",
          customInputActive
            ? "border-transparent bg-muted/65 shadow-sm"
            : "border-border/55 bg-background/35 hover:bg-muted/35",
          disabled && "cursor-not-allowed opacity-60"
        )}
        onMouseDown={(event) => {
          if (disabled) return
          if (event.target instanceof HTMLTextAreaElement) return
          event.preventDefault()
          activateCustomInput()
        }}
      >
        <input
          type={item.multi_select ? "checkbox" : "radio"}
          name={item.id}
          checked={customInputActive}
          disabled={disabled}
          onChange={activateCustomInput}
          className="sr-only"
        />
        <ChoiceIndicator
          checked={customInputActive}
          multiSelect={item.multi_select}
        />
        <span className="min-w-0 flex-1 space-y-0.5">
          <span className="text-ui block font-medium text-pretty text-foreground">
            Use your own answer
          </span>
          {customInputActive ? (
            <textarea
              ref={customTextareaRef}
              value={customText}
              disabled={disabled}
              placeholder={item.placeholder ?? "Type your answer..."}
              rows={1}
              onFocus={activateCustomInput}
              onBlur={() => setCustomInputFocused(false)}
              onInput={(event) => {
                const target = event.currentTarget
                target.style.height = "0px"
                target.style.height = `${target.scrollHeight}px`
              }}
              onChange={(event) =>
                onChange({
                  question_id: item.id,
                  selected_option_ids: [],
                  text: event.target.value,
                })
              }
              className="text-ui min-h-5 w-full resize-none border-0 bg-transparent p-0 leading-5 text-foreground outline-none placeholder:text-muted-foreground/55 focus-visible:ring-0"
            />
          ) : (
            <span className="text-ui block text-pretty text-muted-foreground/80">
              {item.placeholder ?? "Type your answer..."}
            </span>
          )}
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
      placeholder={item.placeholder ?? "Type your answer..."}
      onChange={(event) =>
        onChange({
          question_id: item.id,
          selected_option_ids: [],
          text: event.target.value,
        })
      }
      rows={3}
      className="text-ui min-h-[84px] resize-y rounded-md border-border/55 bg-background/35 px-2.5 py-2 leading-5 placeholder:text-muted-foreground/55"
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
  if (!answer) return `Please answer “${item.question}”.`
  const hasText = Boolean((answer.text ?? "").trim())
  if (item.kind === "text" && !hasText) {
    return `Please answer “${item.question}”.`
  }
  if (
    item.kind !== "text" &&
    answer.selected_option_ids.length === 0 &&
    !hasText
  ) {
    return `Please answer “${item.question}”.`
  }
  return null
}

function questionGuidance(item: QuestionItem): string {
  if (item.kind === "text") {
    return "Type a short answer in your own words."
  }
  if (item.multi_select) {
    return "Select one or more options, or type your own answer."
  }
  return "Select one option, or type your own answer."
}

function answerIsPresent(answer: QuestionAnswer | undefined): boolean {
  if (!answer) return false
  if (answer.selected_option_ids.length > 0) return true
  return (answer.text ?? "").trim().length > 0
}

export function PendingQuestionComposer() {
  const activeSessionId = useChatStore((state) => state.activeSessionId)
  const pendingQuestion = usePendingQuestionStore(
    (state) => state.pendingQuestion
  )
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
  const activeRequestId = pendingQuestion?.request_id

  const validationError = useMemo(() => {
    if (!pendingQuestion) return null
    for (const item of pendingQuestion.questions) {
      if (!item.required) continue
      const answer = answers[item.id]
      if (!answer) return `Please answer “${item.question}”.`
      const hasText = Boolean((answer.text ?? "").trim())
      if (item.kind === "text" && !hasText) {
        return `Please answer “${item.question}”.`
      }
      if (
        item.kind !== "text" &&
        answer.selected_option_ids.length === 0 &&
        !hasText
      ) {
        return `Please answer “${item.question}”.`
      }
    }
    return null
  }, [answers, pendingQuestion])

  const currentValidationError = useMemo(
    () =>
      questionValidationError(
        currentItem,
        currentItem ? answers[currentItem.id] : undefined
      ),
    [answers, currentItem]
  )

  useEffect(() => {
    if (!activeRequestId) {
      setAnswers({})
      setQuestionIndex(0)
      return
    }
    setAnswers({})
    setQuestionIndex(0)
  }, [activeRequestId])

  if (!pendingQuestion) return null
  if (!currentItem) return null

  async function handleSubmit() {
    if (!activeSessionId || validationError || !pendingQuestion) return
    const currentPendingQuestion = pendingQuestion
    const result: QuestionResult = {
      status: "answered",
      request_id: currentPendingQuestion.request_id,
      answers: questionItems.map(
        (item) =>
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
    setQuestionIndex((current) =>
      Math.min(current + 1, questionItems.length - 1)
    )
  }

  return (
    <div className="relative shrink-0 border-t border-border/30 px-4 pt-3 pb-4">
      <div className="pointer-events-none absolute -top-10 right-0 left-0 h-10 bg-gradient-to-t from-background to-transparent" />

      <div className="mx-auto max-w-[720px]">
        <div className="rounded-xl border border-border/50 bg-card px-4 py-3">
          <header className="flex items-center justify-between gap-3 px-2">
            <p className="text-ui font-medium text-muted-foreground/85 tabular-nums">
              {questionIndex + 1} of {questionItems.length}
            </p>
            <div className="flex items-center gap-2">
              {questionItems.map((item, index) => {
                const active = index === questionIndex
                const answered = answerIsPresent(answers[item.id])

                return (
                  <button
                    key={item.id}
                    type="button"
                    disabled={disabled}
                    onClick={() => setQuestionIndex(index)}
                    className={cn(
                      "inline-flex h-4 w-4 items-center justify-center rounded-full",
                      disabled && "cursor-not-allowed opacity-60"
                    )}
                    aria-label={`Question ${index + 1}`}
                  >
                    <span
                      className={cn(
                        "h-0.5 w-4 rounded-full bg-muted-foreground/35",
                        answered && "bg-foreground/55",
                        active && "bg-foreground"
                      )}
                    />
                  </button>
                )
              })}
            </div>
          </header>

          <section key={currentItem.id} className="mt-2 space-y-2.5">
            <h3 className="text-ui px-2 font-medium text-pretty text-foreground">
              {currentItem.question}
            </h3>
            <p className="text-meta px-2 text-pretty text-muted-foreground/75">
              {questionGuidance(currentItem)}
            </p>

            {currentItem.kind === "choice" ? (
              <ChoiceQuestion
                item={currentItem}
                disabled={disabled}
                value={answers[currentItem.id]}
                onChange={(next) =>
                  setAnswers((current) => ({
                    ...current,
                    [currentItem.id]: next,
                  }))
                }
              />
            ) : null}

            {currentItem.kind === "text" ? (
              <TextQuestion
                item={currentItem}
                disabled={disabled}
                value={answers[currentItem.id]}
                onChange={(next) =>
                  setAnswers((current) => ({
                    ...current,
                    [currentItem.id]: next,
                  }))
                }
              />
            ) : null}

            {currentItem.kind === "confirm" ? (
              <ConfirmQuestion
                item={currentItem}
                disabled={disabled}
                value={answers[currentItem.id]}
                onChange={(next) =>
                  setAnswers((current) => ({
                    ...current,
                    [currentItem.id]: next,
                  }))
                }
              />
            ) : null}
          </section>

          <div className="mt-3 border-t border-border/50 px-2 pt-2.5">
            {storeError ? (
              <p
                role="alert"
                className="text-meta text-pretty text-destructive/85"
              >
                {storeError}
              </p>
            ) : null}

            <div
              className={cn(
                "flex flex-wrap items-center justify-between gap-2",
                storeError && "mt-2"
              )}
            >
              <Button
                type="button"
                variant="ghost"
                size="default"
                disabled={disabled}
                onClick={() => void handleDismiss()}
                className="px-2 text-muted-foreground/75"
                title="Ignore remaining questions"
              >
                Dismiss
              </Button>

              <div className="flex items-center gap-2">
                {questionIndex > 0 ? (
                  <Button
                    type="button"
                    variant="secondary"
                    size="default"
                    disabled={disabled}
                    onClick={() =>
                      setQuestionIndex((current) => Math.max(current - 1, 0))
                    }
                  >
                    Back
                  </Button>
                ) : null}

                <Button
                  type="button"
                  variant={isLastQuestion ? "default" : "secondary"}
                  size="default"
                  disabled={disabled || currentValidationError != null}
                  onClick={() => void handlePrimaryAction()}
                  className="min-w-20"
                >
                  {submitting
                    ? "Submitting..."
                    : isLastQuestion
                      ? "Submit"
                      : "Next"}
                </Button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
