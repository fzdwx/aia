import type { QuestionItem, QuestionOption } from "@/lib/types"

export function normalizeQuestionOptions(item: QuestionItem): QuestionOption[] {
  return Array.isArray(item.options) ? item.options : []
}

export function buildConfirmQuestionItem(item: QuestionItem): QuestionItem {
  const options = normalizeQuestionOptions(item)
  return {
    ...item,
    kind: "choice",
    multi_select: false,
    options:
      options.length > 0
        ? options
        : [
            { id: "yes", label: "Yes", description: null },
            { id: "no", label: "No", description: null },
          ],
  }
}
