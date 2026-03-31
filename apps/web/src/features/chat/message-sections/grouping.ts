import type {
  StreamingToolOutput,
  StreamingTurn,
  TurnBlock,
  TurnLifecycle,
} from "@/lib/types"

import { isContextExplorationTool } from "@/features/chat/tool-timeline-helpers"

export type BlockGroup =
  | { type: "single"; block: TurnBlock }
  | { type: "tools"; invocations: TurnLifecycle["tool_invocations"] }

export type StreamingBlockGroup =
  | { type: "thinking"; content: string }
  | { type: "text"; content: string }
  | { type: "tools"; tools: StreamingToolOutput[] }

export function groupBlocks(blocks: TurnBlock[]): BlockGroup[] {
  const result: BlockGroup[] = []

  for (const block of blocks) {
    if (block.kind === "tool_invocation") {
      const last = result[result.length - 1]
      const isContextTool = isContextExplorationTool(
        block.invocation.call.tool_name
      )
      const lastInvocation =
        last && last.type === "tools"
          ? last.invocations[last.invocations.length - 1]
          : null
      const canAppendToContextGroup =
        lastInvocation != null &&
        isContextTool &&
        isContextExplorationTool(lastInvocation.call.tool_name)

      if (canAppendToContextGroup && last && last.type === "tools") {
        last.invocations.push(block.invocation)
      } else {
        result.push({ type: "tools", invocations: [block.invocation] })
      }
    } else {
      result.push({ type: "single", block })
    }
  }

  return result
}

export function groupStreamingBlocks(
  blocks: StreamingTurn["blocks"]
): StreamingBlockGroup[] {
  const groups: StreamingBlockGroup[] = []

  for (const block of blocks) {
    if (block.type === "tool") {
      const last = groups[groups.length - 1]
      const isContextTool = isContextExplorationTool(block.tool.toolName)
      const lastTool =
        last && last.type === "tools" ? last.tools[last.tools.length - 1] : null
      const canAppendToContextGroup =
        lastTool != null &&
        isContextTool &&
        isContextExplorationTool(lastTool.toolName)

      if (canAppendToContextGroup && last && last.type === "tools") {
        last.tools.push(block.tool)
      } else {
        groups.push({ type: "tools", tools: [block.tool] })
      }
    } else {
      groups.push(block)
    }
  }

  return groups
}
