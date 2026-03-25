import {
  asArray,
  asRecord,
  asString,
  extractTraceText,
} from "@/lib/trace-inspection"
import type { TraceRecord } from "@/lib/types"

export function collectSystemPrompts(trace: TraceRecord | null) {
  if (!trace) return []

  const request = asRecord(trace.provider_request)
  if (!request) return []

  const prompts: string[] = []
  const instructions = asString(request.instructions)
  if (instructions) {
    prompts.push(instructions)
  }

  const messages = asArray(request.messages)
  for (const item of messages) {
    const record = asRecord(item)
    if (record?.role === "system") {
      const content = extractTraceText(record.content)
      if (content) prompts.push(content)
    }
  }

  const input = asArray(request.input)
  for (const item of input) {
    const record = asRecord(item)
    if (record?.role === "system") {
      const content = extractTraceText(record.content)
      if (content) prompts.push(content)
    }
  }

  return prompts
}

export function collectToolNames(trace: TraceRecord | null) {
  if (!trace) return []

  const requestSummary = asRecord(trace.request_summary)
  const explicit = asArray(requestSummary?.tool_names).filter(
    (value): value is string => typeof value === "string"
  )
  if (explicit.length > 0) return explicit

  const request = asRecord(trace.provider_request)
  return asArray(request?.tools)
    .map((tool) => {
      const record = asRecord(tool)
      const fn = asRecord(record?.function)
      return asString(record?.name) ?? asString(fn?.name)
    })
    .filter((value): value is string => Boolean(value))
}

export function collectAssistantPreview(trace: TraceRecord | null) {
  if (!trace) return null

  const summary = asRecord(trace.response_summary)
  const assistantText =
    asString(summary?.assistant_text) ||
    extractTraceText(summary?.assistant_text)

  if (assistantText) return assistantText

  return extractAssistantPreviewFromResponseBody(trace.response_body)
}

function extractAssistantPreviewFromResponseBody(body: string | null) {
  if (!body) return null

  const parsedSse = extractAssistantPreviewFromSseBody(body)
  if (parsedSse) return parsedSse

  const parsedJson = extractAssistantPreviewFromJsonBody(body)
  if (parsedJson) return parsedJson

  return null
}

function extractAssistantPreviewFromJsonBody(body: string) {
  try {
    const payload = JSON.parse(body)
    const texts = collectAssistantTextsFromPayload(payload)
    return texts.length > 0 ? texts.join("\n") : null
  } catch {
    return null
  }
}

function extractAssistantPreviewFromSseBody(body: string) {
  const lines = body.split("\n")
  const outputChunks: string[] = []
  const completedPayloadTexts: string[] = []

  for (const line of lines) {
    const data = line.startsWith("data: ") ? line.slice(6) : null
    if (!data || data === "[DONE]") continue

    try {
      const event = JSON.parse(data)
      const type = asString(asRecord(event)?.type)

      if (type === "response.output_text.delta") {
        const text = extractTraceText(asRecord(event)?.delta)
        if (text) outputChunks.push(text)
        continue
      }

      if (type === "response.output_text.done" && outputChunks.length === 0) {
        const text = extractTraceText(asRecord(event)?.text)
        if (text) outputChunks.push(text)
        continue
      }

      if (type === "response.completed") {
        const response = asRecord(event)?.response
        const texts = collectAssistantTextsFromPayload(response)
        if (texts.length > 0) {
          completedPayloadTexts.push(...texts)
        }
      }
    } catch {
      continue
    }
  }

  if (outputChunks.length > 0) {
    return outputChunks.join("").trim() || null
  }

  if (completedPayloadTexts.length > 0) {
    return completedPayloadTexts.join("\n").trim() || null
  }

  return null
}

function collectAssistantTextsFromPayload(payload: unknown): string[] {
  const record = asRecord(payload)
  if (!record) return []

  const texts: string[] = []
  const push = (value: unknown) => {
    const text = extractTraceText(value).trim()
    if (text && !texts.includes(text)) {
      texts.push(text)
    }
  }

  const choices = asArray(record.choices)
  for (const choice of choices) {
    const message = asRecord(asRecord(choice)?.message)
    if (!message) continue
    push(message.content)
  }

  const output = asArray(record.output)
  for (const item of output) {
    const outputItem = asRecord(item)
    if (!outputItem) continue
    if (
      asString(outputItem.role) &&
      asString(outputItem.role) !== "assistant"
    ) {
      continue
    }

    if (asString(outputItem.type) === "message") {
      push(outputItem.content)
      continue
    }

    push(outputItem.content)
  }

  return texts
}
