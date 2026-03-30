import { describe, expect, test } from "vite-plus/test"

import { applyStreamEventToBlocks } from "./chat-sse-projection"

describe("chat sse projection", () => {
  test("preserves shell output segments for live rendering", () => {
    const blocks = applyStreamEventToBlocks(
      [
        {
          type: "tool",
          tool: {
            invocationId: "shell-1",
            toolName: "Shell",
            arguments: { command: "cargo test --workspace" },
            detectedAtMs: 1,
            startedAtMs: 1,
            output: "",
            completed: false,
          },
        },
      ],
      {
        kind: "tool_output_delta",
        invocation_id: "shell-1",
        stream: "stderr",
        text: "warning: noisy\n",
        session_id: "session-1",
        turn_id: "turn-1",
      }
    )

    const block = blocks[0]
    expect(block?.type).toBe("tool")
    if (!block || block.type !== "tool") {
      throw new Error("expected first block to stay a tool block")
    }

    expect(block.tool.output).toBe("warning: noisy\n")
    expect(block.tool.outputSegments).toEqual([
      { stream: "stderr", text: "warning: noisy\n" },
    ])
  })
})
