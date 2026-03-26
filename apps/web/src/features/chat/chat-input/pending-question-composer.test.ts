import { describe, expect, test } from "vite-plus/test"

import {
  buildConfirmQuestionItem,
  normalizeQuestionOptions,
} from "./pending-question-helpers"

describe("pending question helpers", () => {
  test("normalizeQuestionOptions returns an empty array when options are missing", () => {
    expect(
      normalizeQuestionOptions({
        id: "confirm-runtime",
        question: "Need confirmation?",
        kind: "confirm",
        required: true,
        multi_select: false,
      })
    ).toEqual([])
  })

  test("buildConfirmQuestionItem injects yes and no fallback options", () => {
    expect(
      buildConfirmQuestionItem({
        id: "confirm-runtime",
        question: "Need confirmation?",
        kind: "confirm",
        required: true,
        multi_select: true,
      })
    ).toMatchObject({
      kind: "choice",
      multi_select: false,
      options: [
        { id: "yes", label: "Yes", description: null },
        { id: "no", label: "No", description: null },
      ],
    })
  })
})
