import type {
  StreamingToolOutput,
  StreamingTurn,
  TurnBlock,
  ToolInvocationLifecycle,
} from "@/lib/types"

import { isContextExplorationTool } from "@/features/chat/tool-timeline-helpers"

export type BlockGroup =
  | { type: "single"; block: TurnBlock }
  | { type: "tools"; invocations: ToolInvocationLifecycle[] }
  | { type: "tool-run"; groups: { invocations: ToolInvocationLifecycle[] }[] }

export type StreamingBlockGroup =
  | { type: "thinking"; content: string }
  | { type: "text"; content: string }
  | { type: "tool-run"; groups: { tools: StreamingToolOutput[] }[] }

export function groupBlocks(blocks: TurnBlock[]): BlockGroup[] {
  // First pass: group consecutive tools by type (context vs non-context)
  const intermediate: (ToolInvocationLifecycle[] | TurnBlock)[] = []

  for (const block of blocks) {
    if (block.kind === "tool_invocation") {
      const last = intermediate[intermediate.length - 1]
      const isContextTool = isContextExplorationTool(block.invocation.call.tool_name)

      // Check if we can append to previous tool group (same type)
      const canAppend =
        Array.isArray(last) &&
        last.length > 0 &&
        isContextExplorationTool(last[0].call.tool_name) === isContextTool

      if (canAppend && Array.isArray(last)) {
        last.push(block.invocation)
      } else {
        intermediate.push([block.invocation])
      }
    } else {
      intermediate.push(block)
    }
  }

  // Second pass: wrap ALL consecutive tool arrays into a single tool-run
  const result: BlockGroup[] = []
  let currentRun: { invocations: ToolInvocationLifecycle[] }[] = []

  const flushRun = () => {
    if (currentRun.length === 1) {
      result.push({ type: "tools", invocations: currentRun[0].invocations })
    } else if (currentRun.length > 1) {
      result.push({ type: "tool-run", groups: currentRun })
    }
    currentRun = []
  }

  for (const item of intermediate) {
    if (Array.isArray(item)) {
      // This is a tool group - add to current run
      currentRun.push({ invocations: item })
    } else {
      // This is a non-tool block - flush the run first
      flushRun()
      result.push({ type: "single", block: item })
    }
  }

  flushRun()
  return result
}

export function groupStreamingBlocks(
  blocks: StreamingTurn["blocks"]
): StreamingBlockGroup[] {
  // First pass: group consecutive tools by type (context vs non-context)
  const intermediate: (StreamingToolOutput[] | { type: "thinking"; content: string } | { type: "text"; content: string })[] = []

  for (const block of blocks) {
    if (block.type === "tool") {
      const last = intermediate[intermediate.length - 1]
      const isContextTool = isContextExplorationTool(block.tool.toolName)

      // Check if we can append to previous tool group (same type)
      const canAppend =
        Array.isArray(last) &&
        last.length > 0 &&
        isContextExplorationTool(last[0].toolName) === isContextTool

      if (canAppend && Array.isArray(last)) {
        last.push(block.tool)
      } else {
        intermediate.push([block.tool])
      }
    } else {
      intermediate.push(block)
    }
  }

  // Second pass: wrap ALL consecutive tool arrays into a single tool-run
  const result: StreamingBlockGroup[] = []
  let currentRun: { tools: StreamingToolOutput[] }[] = []

  const flushRun = () => {
    if (currentRun.length >= 1) {
      result.push({ type: "tool-run", groups: currentRun })
    }
    currentRun = []
  }

  for (const item of intermediate) {
    if (Array.isArray(item)) {
      // This is a tool group - add to current run
      currentRun.push({ tools: item })
    } else {
      // This is a non-tool block - flush the run first
      flushRun()
      result.push(item)
    }
  }

  flushRun()
  return result
}
