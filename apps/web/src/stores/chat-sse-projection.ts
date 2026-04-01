import { normalizeToolArguments } from "@/lib/tool-display"
import type {
  CurrentTurnSnapshot,
  SseEvent,
  StreamingBlock,
  StreamingTurn,
  ToolOutputSegment,
  TurnStatus,
} from "@/lib/types"

type StreamEventData = Extract<SseEvent, { type: "stream" }>["data"]

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
              invocationId: block.tool.invocation_id,
              toolName: block.tool.tool_name,
              arguments: normalizeToolArguments(block.tool.arguments),
              detectedAtMs: block.tool.detected_at_ms,
              startedAtMs: block.tool.started_at_ms ?? undefined,
              finishedAtMs: block.tool.finished_at_ms ?? undefined,
              output: block.tool.output,
              // 已完成的工具不需要 segments，只保留 output 字符串
              outputSegments: undefined,
              completed: block.tool.completed,
              resultContent: block.tool.result_content ?? undefined,
              resultDetails: block.tool.result_details ?? undefined,
              failed: block.tool.failed ?? undefined,
            },
          } as const
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
      nextBlocks[existingIndex] = {
        ...block,
        tool: {
          ...block.tool,
          toolName: data.tool_name || block.tool.toolName,
          arguments: mergedArguments,
        },
      }
    } else {
      nextBlocks.push({
        type: "tool",
        tool: {
          invocationId: data.invocation_id,
          toolName: data.tool_name,
          arguments: normalizeToolArguments(data.arguments),
          detectedAtMs: data.detected_at_ms,
          output: "",
          completed: false,
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
      nextBlocks[existingIndex] = {
        ...block,
        tool: {
          ...block.tool,
          toolName: data.tool_name || block.tool.toolName,
          arguments: mergedArguments,
          startedAtMs: block.tool.startedAtMs ?? data.started_at_ms,
        },
      }
    } else {
      const startedAtMs = data.started_at_ms
      nextBlocks.push({
        type: "tool",
        tool: {
          invocationId: data.invocation_id,
          toolName: data.tool_name,
          arguments: normalizeToolArguments(data.arguments),
          detectedAtMs: startedAtMs,
          startedAtMs,
          output: "",
          completed: false,
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
      // 合并相邻的同类型 segment，避免无限累积
      const existingSegments = block.tool.outputSegments ?? []
      const lastSegment = existingSegments[existingSegments.length - 1]
      let outputSegments: ToolOutputSegment[]

      if (lastSegment && lastSegment.stream === data.stream) {
        // 合并到上一个 segment，避免创建新对象
        outputSegments = [
          ...existingSegments.slice(0, -1),
          { stream: lastSegment.stream, text: lastSegment.text + data.text }
        ]
      } else {
        // 限制最大 segment 数量，防止内存无限增长
        const MAX_SEGMENTS = 200
        outputSegments = [
          ...(existingSegments.length >= MAX_SEGMENTS
            ? existingSegments.slice(-MAX_SEGMENTS + 1)
            : existingSegments),
          { stream: data.stream, text: data.text },
        ]
      }

      nextBlocks[existingIndex] = {
        ...block,
        tool: {
          ...block.tool,
          output: block.tool.output + data.text,
          outputSegments,
        },
      }
    } else {
      const startedAtMs = Date.now()
      nextBlocks.push({
        type: "tool",
        tool: {
          invocationId: data.invocation_id,
          toolName: "",
          arguments: {},
          detectedAtMs: startedAtMs,
          startedAtMs,
          output: data.text,
          outputSegments: [{ stream: data.stream, text: data.text }],
          completed: false,
        },
      })
    }
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
        },
      }
    }
  }

  return nextBlocks
}
