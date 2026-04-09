import { normalizeToolArguments } from "@/lib/tool-display"
import type {
  CurrentTurnSnapshot,
  CurrentUiWidget,
  SseEvent,
  StreamingBlock,
  StreamingUiWidget,
  StreamingTurn,
  ToolOutputSegment,
  TurnStatus,
} from "@/lib/types"

type StreamEventData = Extract<SseEvent, { type: "stream" }>["data"]

const MAX_STREAMING_TOOL_OUTPUT_TAIL_CHARS = 64 * 1024
const MAX_STREAMING_TOOL_OUTPUT_SEGMENTS = 200

function appendOutputTail(existingOutput: string, delta: string): string {
  const combined = existingOutput + delta
  if (combined.length <= MAX_STREAMING_TOOL_OUTPUT_TAIL_CHARS) {
    return combined
  }
  return combined.slice(-MAX_STREAMING_TOOL_OUTPUT_TAIL_CHARS)
}

function appendOutputSegments(
  existingSegments: ToolOutputSegment[] | undefined,
  nextSegment: ToolOutputSegment
): ToolOutputSegment[] {
  const segments = existingSegments ?? []
  const lastSegment = segments[segments.length - 1]

  if (lastSegment && lastSegment.stream === nextSegment.stream) {
    const mergedLastText = appendOutputTail(lastSegment.text, nextSegment.text)
    const mergedLastSegment = {
      stream: lastSegment.stream,
      text: mergedLastText,
    } as const
    if (segments.length === 1) {
      return [mergedLastSegment]
    }
    return [...segments.slice(0, -1), mergedLastSegment]
  }

  const boundedSegment = {
    stream: nextSegment.stream,
    text: appendOutputTail("", nextSegment.text),
  } as const

  if (segments.length >= MAX_STREAMING_TOOL_OUTPUT_SEGMENTS) {
    return [...segments.slice(-MAX_STREAMING_TOOL_OUTPUT_SEGMENTS + 1), boundedSegment]
  }

  return [...segments, boundedSegment]
}

function isWidgetRendererToolName(toolName: string): boolean {
  return toolName === "WidgetRenderer" || toolName === "widgetRenderer"
}

function extractWidgetHtmlFromRawArguments(rawArguments: string): string {
  const htmlKeyIndex = rawArguments.indexOf('"html"')
  if (htmlKeyIndex < 0) {
    return ""
  }
  const firstQuoteIndex = rawArguments.indexOf('"', htmlKeyIndex + 6)
  if (firstQuoteIndex < 0) {
    return ""
  }

  let cursor = firstQuoteIndex + 1
  let escaped = false
  let extracted = ""
  while (cursor < rawArguments.length) {
    const current = rawArguments[cursor]
    if (escaped) {
      switch (current) {
        case "n":
          extracted += "\n"
          break
        case "r":
          extracted += "\r"
          break
        case "t":
          extracted += "\t"
          break
        case '"':
        case "\\":
        case "/":
          extracted += current
          break
        default:
          extracted += current
          break
      }
      escaped = false
      cursor += 1
      continue
    }

    if (current === "\\") {
      escaped = true
      cursor += 1
      continue
    }
    if (current === '"') {
      break
    }
    extracted += current
    cursor += 1
  }

  return extracted.trim()
}

function deriveWidgetPreviewHtml(tool: {
  toolName: string
  output: string
  outputSegments?: ToolOutputSegment[]
  rawArguments?: string
  resultDetails?: Record<string, unknown>
  widget?: StreamingUiWidget
}): string | undefined {
  if (!isWidgetRendererToolName(tool.toolName)) {
    return undefined
  }

  if (tool.widget?.document.html.trim()) {
    return tool.widget.document.html.trim()
  }

  if (
    typeof tool.resultDetails?.html === "string" &&
    tool.resultDetails.html.trim().length > 0
  ) {
    return tool.resultDetails.html.trim()
  }

  const stdout = (tool.outputSegments ?? [])
    .filter((segment) => segment.stream === "stdout")
    .map((segment) => segment.text)
    .join("")
    .trim()
  if (stdout.length > 0) {
    return stdout
  }

  const output = tool.output.trim()
  if (output.length > 0) {
    return output
  }

  const rawArguments = tool.rawArguments ?? ""
  const html = extractWidgetHtmlFromRawArguments(rawArguments)
  return html.length > 0 ? html : undefined
}

function deriveStreamingWidget(tool: {
  invocationId: string
  toolName: string
  arguments: Record<string, unknown>
  rawArguments?: string
  output: string
  outputSegments?: ToolOutputSegment[]
  resultDetails?: Record<string, unknown>
  completed: boolean
  widget?: StreamingUiWidget
}): StreamingUiWidget | undefined {
  if (!isWidgetRendererToolName(tool.toolName)) {
    return undefined
  }

  if (
    tool.widget &&
    tool.widget.phase === (tool.completed ? "final" : "preview")
  ) {
    return tool.widget
  }

  const details = tool.resultDetails ?? undefined
  const title =
    (typeof details?.title === "string" && details.title.trim().length > 0
      ? details.title.trim()
      : undefined) ??
    (typeof tool.arguments.title === "string" &&
    tool.arguments.title.trim().length > 0
      ? tool.arguments.title.trim()
      : undefined) ??
    "Widget"
  const description =
    (typeof details?.description === "string"
      ? details.description
      : undefined) ??
    (typeof tool.arguments.description === "string"
      ? tool.arguments.description
      : undefined) ??
    ""
  const html =
    (typeof details?.html === "string" && details.html.trim().length > 0
      ? details.html.trim()
      : undefined) ?? deriveWidgetPreviewHtml(tool)

  if (!html || html.length === 0) {
    return undefined
  }

  const contentType =
    (typeof details?.content_type === "string" &&
    details.content_type.trim().length > 0
      ? details.content_type.trim()
      : undefined) ?? "text/html"

  return {
    instanceId: tool.invocationId,
    phase: tool.completed ? "final" : "preview",
    document: {
      title,
      description,
      html,
      contentType,
    },
  }
}

function mapCurrentWidget(
  widget: CurrentUiWidget | undefined
): StreamingUiWidget | undefined {
  if (!widget) {
    return undefined
  }

  return {
    instanceId: widget.instance_id,
    phase: widget.phase,
    document: {
      title: widget.document.title,
      description: widget.document.description,
      html: widget.document.html,
      contentType: widget.document.content_type,
    },
  }
}

function mapHostCommandWidget(
  command: Extract<StreamEventData, { kind: "widget_host_command" }>["command"]
): StreamingUiWidget | undefined {
  if (command.type !== "render") {
    return undefined
  }

  return mapCurrentWidget(command.widget)
}

function mapStreamingWidget(
  widget: StreamingUiWidget | undefined
): StreamingUiWidget | undefined {
  return widget
}

function withWidgetProjection<
  T extends {
    invocationId: string
    toolName: string
    arguments: Record<string, unknown>
    output: string
    outputSegments?: ToolOutputSegment[]
    rawArguments?: string
    resultDetails?: Record<string, unknown>
    completed: boolean
    widget?: StreamingUiWidget
  },
>(tool: T): T & { previewHtml?: string; widget?: StreamingUiWidget } {
  const previewHtml = deriveWidgetPreviewHtml(tool)
  const widget = deriveStreamingWidget(tool)
  if (previewHtml == null && widget == null) {
    return tool
  }

  return {
    ...tool,
    ...(previewHtml == null ? {} : { previewHtml }),
    ...(widget == null ? {} : { widget }),
  }
}

function tryParseRawToolArguments(
  rawArguments: string
): Record<string, unknown> | null {
  try {
    const parsed = JSON.parse(rawArguments) as unknown
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return null
    }
    return normalizeToolArguments(parsed as Record<string, unknown>)
  } catch {
    return null
  }
}

function isPlaceholderToolMetadata(
  tool: Extract<StreamingBlock, { type: "tool" }>["tool"]
): boolean {
  return (
    tool.toolName.trim().length === 0 &&
    Object.keys(tool.arguments).length === 0
  )
}

function findToolBlockIndex(
  blocks: StreamingBlock[],
  invocationId: string
): number {
  for (let index = blocks.length - 1; index >= 0; index -= 1) {
    const block = blocks[index]
    if (block?.type === "tool" && block.tool.invocationId === invocationId) {
      return index
    }
  }
  return -1
}

export function currentTurnToStreamingTurn(
  current: CurrentTurnSnapshot
): StreamingTurn {
  return {
    userMessages: current.user_messages ?? [],
    status: current.status,
    blocks: current.blocks.map((block) => {
      switch (block.kind) {
        case "thinking":
          return { type: "thinking", content: block.content } as const
        case "text":
          return { type: "text", content: block.content } as const
        case "tool":
          return {
            type: "tool",
            tool: {
              ...withWidgetProjection({
                invocationId: block.tool.invocation_id,
                toolName: block.tool.tool_name,
                arguments: normalizeToolArguments(block.tool.arguments),
                rawArguments: block.tool.raw_arguments ?? undefined,
                detectedAtMs: block.tool.detected_at_ms,
                startedAtMs: block.tool.started_at_ms ?? undefined,
                finishedAtMs: block.tool.finished_at_ms ?? undefined,
                output: block.tool.output,
                outputSegments: block.tool.output_segments ?? undefined,
                completed: block.tool.completed,
                resultContent: block.tool.result_content ?? undefined,
                resultDetails: block.tool.result_details ?? undefined,
                failed: block.tool.failed ?? undefined,
                widget: mapCurrentWidget(block.tool.widget),
              }),
            },
          } as const
        default:
          throw new Error(
            `unsupported current turn block kind: ${JSON.stringify(block)}`
          )
      }
    }),
  }
}

export function createPendingStreamingTurn(prompts: string[]): StreamingTurn {
  return {
    userMessages: prompts,
    status: "waiting",
    blocks: [],
  }
}

export function withStreamingStatus(
  streamingTurn: StreamingTurn,
  status: TurnStatus
): StreamingTurn {
  return {
    ...streamingTurn,
    status,
  }
}

export function applyStreamEventToBlocks(
  blocks: StreamingBlock[],
  data: StreamEventData
): StreamingBlock[] {
  const nextBlocks = [...blocks]

  if (data.kind === "thinking_delta") {
    const last = nextBlocks[nextBlocks.length - 1]
    if (last && last.type === "thinking") {
      nextBlocks[nextBlocks.length - 1] = {
        ...last,
        content: last.content + data.text,
      }
    } else {
      nextBlocks.push({ type: "thinking", content: data.text })
    }
    return nextBlocks
  }

  if (data.kind === "text_delta") {
    const last = nextBlocks[nextBlocks.length - 1]
    if (last && last.type === "text") {
      nextBlocks[nextBlocks.length - 1] = {
        ...last,
        content: last.content + data.text,
      }
    } else {
      nextBlocks.push({ type: "text", content: data.text })
    }
    return nextBlocks
  }

  if (data.kind === "tool_call_detected") {
    const existingIndex = findToolBlockIndex(nextBlocks, data.invocation_id)
    if (existingIndex >= 0) {
      const block = nextBlocks[existingIndex] as Extract<
        StreamingBlock,
        { type: "tool" }
      >
      const mergedArguments = {
        ...block.tool.arguments,
        ...normalizeToolArguments(data.arguments),
      }
      const shouldRepairPlaceholder = isPlaceholderToolMetadata(block.tool)
      nextBlocks[existingIndex] = {
        ...block,
        tool: {
          ...withWidgetProjection({
            ...block.tool,
            toolName: data.tool_name || block.tool.toolName,
            arguments: mergedArguments,
            rawArguments:
              block.tool.rawArguments ??
              JSON.stringify(normalizeToolArguments(data.arguments ?? {})),
            detectedAtMs: shouldRepairPlaceholder
              ? data.detected_at_ms
              : block.tool.detectedAtMs,
            widget: mapStreamingWidget(data.widget) ?? block.tool.widget,
          }),
        },
      }
    } else {
      nextBlocks.push({
        type: "tool",
        tool: {
          ...withWidgetProjection({
            invocationId: data.invocation_id,
            toolName: data.tool_name,
            arguments: normalizeToolArguments(data.arguments),
            rawArguments: JSON.stringify(
              normalizeToolArguments(data.arguments ?? {})
            ),
            detectedAtMs: data.detected_at_ms,
            output: "",
            completed: false,
            widget: mapStreamingWidget(data.widget),
          }),
        },
      })
    }
    return nextBlocks
  }

  if (data.kind === "tool_call_arguments_delta") {
    const existingIndex = findToolBlockIndex(nextBlocks, data.invocation_id)
    if (existingIndex >= 0) {
      const block = nextBlocks[existingIndex] as Extract<
        StreamingBlock,
        { type: "tool" }
      >
      const rawArguments =
        (block.tool.rawArguments ?? "") + data.arguments_delta
      const parsedArguments = tryParseRawToolArguments(rawArguments)
      nextBlocks[existingIndex] = {
        ...block,
        tool: {
          ...withWidgetProjection({
            ...block.tool,
            toolName: data.tool_name || block.tool.toolName,
            arguments: parsedArguments ?? block.tool.arguments,
            rawArguments,
            widget: mapStreamingWidget(data.widget) ?? block.tool.widget,
          }),
        },
      }
    } else {
      const rawArguments = data.arguments_delta
      nextBlocks.push({
        type: "tool",
        tool: {
          ...withWidgetProjection({
            invocationId: data.invocation_id,
            toolName: data.tool_name,
            arguments: tryParseRawToolArguments(rawArguments) ?? {},
            rawArguments,
            detectedAtMs: Date.now(),
            output: "",
            completed: false,
            widget: mapStreamingWidget(data.widget),
          }),
        },
      })
    }
    return nextBlocks
  }

  if (data.kind === "tool_call_started") {
    const existingIndex = findToolBlockIndex(nextBlocks, data.invocation_id)
    if (existingIndex >= 0) {
      const block = nextBlocks[existingIndex] as Extract<
        StreamingBlock,
        { type: "tool" }
      >
      const mergedArguments = {
        ...block.tool.arguments,
        ...normalizeToolArguments(data.arguments),
      }
      const shouldRepairPlaceholder = isPlaceholderToolMetadata(block.tool)
      nextBlocks[existingIndex] = {
        ...block,
        tool: {
          ...withWidgetProjection({
            ...block.tool,
            toolName: data.tool_name || block.tool.toolName,
            arguments: mergedArguments,
            rawArguments:
              block.tool.rawArguments ??
              JSON.stringify(normalizeToolArguments(data.arguments ?? {})),
            detectedAtMs: shouldRepairPlaceholder
              ? data.started_at_ms
              : block.tool.detectedAtMs,
            startedAtMs:
              shouldRepairPlaceholder || block.tool.startedAtMs == null
                ? data.started_at_ms
                : block.tool.startedAtMs,
            widget: mapStreamingWidget(data.widget) ?? block.tool.widget,
          }),
        },
      }
    } else {
      const startedAtMs = data.started_at_ms
      nextBlocks.push({
        type: "tool",
        tool: {
          ...withWidgetProjection({
            invocationId: data.invocation_id,
            toolName: data.tool_name,
            arguments: normalizeToolArguments(data.arguments),
            rawArguments: JSON.stringify(
              normalizeToolArguments(data.arguments ?? {})
            ),
            detectedAtMs: startedAtMs,
            startedAtMs,
            output: "",
            completed: false,
            widget: mapStreamingWidget(data.widget),
          }),
        },
      })
    }
    return nextBlocks
  }

  if (data.kind === "tool_output_delta") {
    const existingIndex = findToolBlockIndex(nextBlocks, data.invocation_id)
    if (existingIndex >= 0) {
      const block = nextBlocks[existingIndex] as Extract<
        StreamingBlock,
        { type: "tool" }
      >
      const outputSegments = appendOutputSegments(block.tool.outputSegments, {
        stream: data.stream,
        text: data.text,
      })
      const nextOutput = appendOutputTail(block.tool.output, data.text)
      const nextWidget = mapStreamingWidget(data.widget)

      nextBlocks[existingIndex] = {
        ...block,
        tool: {
          ...withWidgetProjection({
            ...block.tool,
            output: nextOutput,
            outputSegments,
            widget:
              nextWidget ??
              (block.tool.widget &&
              isWidgetRendererToolName(block.tool.toolName) &&
              data.stream === "stdout"
                ? {
                    ...block.tool.widget,
                    document: {
                      ...block.tool.widget.document,
                      html: outputSegments
                        .filter((segment) => segment.stream === "stdout")
                        .map((segment) => segment.text)
                        .join(""),
                    },
                  }
                : block.tool.widget),
          }),
        },
      }
    } else {
      const startedAtMs = Date.now()
      nextBlocks.push({
        type: "tool",
        tool: {
          ...withWidgetProjection({
            invocationId: data.invocation_id,
            toolName: "",
            arguments: {},
            detectedAtMs: startedAtMs,
            startedAtMs,
            output: appendOutputTail("", data.text),
            outputSegments: [{ stream: data.stream, text: data.text }],
            completed: false,
            widget: mapStreamingWidget(data.widget),
          }),
        },
      })
    }
    return nextBlocks
  }

  if (data.kind === "widget_host_command") {
    const nextWidget =
      mapHostCommandWidget(data.command) ?? mapStreamingWidget(data.widget)
    if (!nextWidget) {
      return nextBlocks
    }

    const existingIndex = findToolBlockIndex(nextBlocks, data.invocation_id)
    if (existingIndex < 0) {
      return nextBlocks
    }

    const block = nextBlocks[existingIndex] as Extract<
      StreamingBlock,
      { type: "tool" }
    >
    nextBlocks[existingIndex] = {
      ...block,
      tool: {
        ...withWidgetProjection({
          ...block.tool,
          widget: nextWidget,
          previewHtml: nextWidget.document.html,
        }),
      },
    }
    return nextBlocks
  }

  if (data.kind === "widget_client_event") {
    return nextBlocks
  }

  if (data.kind === "tool_call_completed") {
    const existingIndex = findToolBlockIndex(nextBlocks, data.invocation_id)
    if (existingIndex >= 0) {
      const block = nextBlocks[existingIndex] as Extract<
        StreamingBlock,
        { type: "tool" }
      >
      nextBlocks[existingIndex] = {
        ...block,
        tool: {
          ...withWidgetProjection({
            ...block.tool,
            startedAtMs:
              block.tool.startedAtMs ??
              (block.tool.detectedAtMs > 0
                ? block.tool.detectedAtMs
                : data.finished_at_ms),
            finishedAtMs: data.finished_at_ms,
            completed: true,
            resultContent: data.content,
            resultDetails: data.details,
            failed: data.failed,
            // 工具完成后清理 segments，只保留 output 字符串，释放内存
            outputSegments: undefined,
            widget: mapStreamingWidget(data.widget) ?? block.tool.widget,
          }),
        },
      }
    }
  }

  return nextBlocks
}
