import { describe, expect, test } from "vite-plus/test"

import {
  applyStreamEventToBlocks,
  currentTurnToStreamingTurn,
} from "./chat-sse-projection"

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

  test("caps streaming tool output tail while preserving live segments", () => {
    const largeChunk = "x".repeat(70_000)
    const blocks = applyStreamEventToBlocks(
      [
        {
          type: "tool",
          tool: {
            invocationId: "shell-tail-1",
            toolName: "Shell",
            arguments: { command: "cat huge.log" },
            detectedAtMs: 1,
            startedAtMs: 1,
            output: "prefix-",
            completed: false,
          },
        },
      ],
      {
        kind: "tool_output_delta",
        invocation_id: "shell-tail-1",
        stream: "stdout",
        text: largeChunk,
        session_id: "session-1",
        turn_id: "turn-1",
      }
    )

    const block = blocks[0]
    expect(block?.type).toBe("tool")
    if (!block || block.type !== "tool") {
      throw new Error("expected first block to stay a tool block")
    }

    expect(block.tool.output.length).toBeLessThanOrEqual(64 * 1024)
    expect(block.tool.output.endsWith(largeChunk.slice(-(64 * 1024)))).toBe(true)
    expect(block.tool.outputSegments).toEqual([
      { stream: "stdout", text: largeChunk.slice(-(64 * 1024)) },
    ])
  })

  test("accumulates widget renderer output for live preview", () => {
    const blocks = applyStreamEventToBlocks(
      [
        {
          type: "tool",
          tool: {
            invocationId: "widget-1",
            toolName: "WidgetRenderer",
            arguments: {
              title: "流式 widget",
              description: "live preview",
            },
            detectedAtMs: 1,
            startedAtMs: 1,
            output: "",
            completed: false,
          },
        },
      ],
      {
        kind: "tool_output_delta",
        invocation_id: "widget-1",
        stream: "stdout",
        text: '<div class="card">live</div>',
        widget: {
          instanceId: "widget-1",
          phase: "preview",
          document: {
            title: "流式 widget",
            description: "live preview",
            html: '<div class="card">live</div>',
            contentType: "text/html",
          },
        },
        session_id: "session-1",
        turn_id: "turn-1",
      }
    )

    const block = blocks[0]
    expect(block?.type).toBe("tool")
    if (!block || block.type !== "tool") {
      throw new Error("expected first block to stay a tool block")
    }

    expect(block.tool.output).toBe('<div class="card">live</div>')
    expect(block.tool.outputSegments).toEqual([
      { stream: "stdout", text: '<div class="card">live</div>' },
    ])
    expect(block.tool.previewHtml).toBe('<div class="card">live</div>')
    expect(block.tool.widget).toEqual({
      instanceId: "widget-1",
      phase: "preview",
      document: {
        title: "流式 widget",
        description: "live preview",
        html: '<div class="card">live</div>',
        contentType: "text/html",
      },
    })
  })

  test("prefers widget payload html over truncated output tail", () => {
    const hugeHtml = `<div>${"x".repeat(70_000)}</div>`
    const blocks = applyStreamEventToBlocks(
      [
        {
          type: "tool",
          tool: {
            invocationId: "widget-tail-1",
            toolName: "WidgetRenderer",
            arguments: {
              title: "流式 widget",
              description: "large html preview",
            },
            detectedAtMs: 1,
            startedAtMs: 1,
            output: "",
            completed: false,
          },
        },
      ],
      {
        kind: "tool_output_delta",
        invocation_id: "widget-tail-1",
        stream: "stdout",
        text: hugeHtml,
        widget: {
          instanceId: "widget-tail-1",
          phase: "preview",
          document: {
            title: "流式 widget",
            description: "large html preview",
            html: hugeHtml,
            contentType: "text/html",
          },
        },
        session_id: "session-1",
        turn_id: "turn-1",
      }
    )

    const block = blocks[0]
    expect(block?.type).toBe("tool")
    if (!block || block.type !== "tool") {
      throw new Error("expected first block to stay a tool block")
    }

    expect(block.tool.output.length).toBeLessThanOrEqual(64 * 1024)
    expect(block.tool.previewHtml).toBe(hugeHtml)
    expect(block.tool.widget?.document.html).toBe(hugeHtml)
  })

  test("accumulates raw tool arguments for widget parameter streaming", () => {
    const blocks = applyStreamEventToBlocks(
      [
        {
          type: "tool",
          tool: {
            invocationId: "widget-args-1",
            toolName: "WidgetRenderer",
            arguments: {
              title: "流式 widget",
              description: "参数流",
            },
            detectedAtMs: 1,
            output: "",
            completed: false,
          },
        },
      ],
      {
        kind: "tool_call_arguments_delta",
        invocation_id: "widget-args-1",
        tool_name: "WidgetRenderer",
        arguments_delta:
          '{"title":"流式 widget","description":"参数流","html":"<div class=\\"card\\">li',
        session_id: "session-1",
        turn_id: "turn-1",
      }
    )

    const block = blocks[0]
    expect(block?.type).toBe("tool")
    if (!block || block.type !== "tool") {
      throw new Error("expected first block to stay a tool block")
    }

    expect(block.tool.rawArguments).toBe(
      '{"title":"流式 widget","description":"参数流","html":"<div class=\\"card\\">li'
    )
    expect(block.tool.arguments).toEqual({
      title: "流式 widget",
      description: "参数流",
    })
    expect(block.tool.previewHtml).toBe('<div class="card">li')
    expect(block.tool.widget).toEqual({
      instanceId: "widget-args-1",
      phase: "preview",
      document: {
        title: "流式 widget",
        description: "参数流",
        html: '<div class="card">li',
        contentType: "text/html",
      },
    })
  })

  test("applies widget host command render payload to existing tool block", () => {
    const blocks = applyStreamEventToBlocks(
      [
        {
          type: "tool",
          tool: {
            invocationId: "widget-host-1",
            toolName: "WidgetRenderer",
            arguments: {
              title: "旧 widget",
            },
            detectedAtMs: 1,
            output: "",
            completed: false,
          },
        },
      ],
      {
        kind: "widget_host_command",
        invocation_id: "widget-host-1",
        command: {
          type: "render",
          widget: {
            instance_id: "widget-host-1",
            phase: "preview",
            document: {
              title: "共享 widget",
              description: "来自后端 host command",
              html: '<div class="card">shared</div>',
              content_type: "text/html",
            },
          },
        },
        session_id: "session-1",
        turn_id: "turn-1",
      }
    )

    const block = blocks[0]
    expect(block?.type).toBe("tool")
    if (!block || block.type !== "tool") {
      throw new Error("expected first block to stay a tool block")
    }

    expect(block.tool.previewHtml).toBe('<div class="card">shared</div>')
    expect(block.tool.widget).toEqual({
      instanceId: "widget-host-1",
      phase: "preview",
      document: {
        title: "共享 widget",
        description: "来自后端 host command",
        html: '<div class="card">shared</div>',
        contentType: "text/html",
      },
    })
  })

  test("hydrates current turn widget output segments for live preview", () => {
    const streaming = currentTurnToStreamingTurn({
      started_at_ms: 1,
      user_messages: ["渲染 widget"],
      status: "working",
      blocks: [
        {
          kind: "tool",
          tool: {
            invocation_id: "widget-1",
            tool_name: "WidgetRenderer",
            arguments: {
              title: "流式 widget",
              description: "来自 current-turn 快照",
            },
            detected_at_ms: 1,
            started_at_ms: 1,
            finished_at_ms: null,
            output: '<div class="card">live</div>warn: noisy',
            output_segments: [
              { stream: "stdout", text: '<div class="card">live</div>' },
              { stream: "stderr", text: "warn: noisy" },
            ],
            completed: false,
            result_content: null,
            result_details: null,
            failed: null,
          },
        },
      ],
    })

    const block = streaming.blocks[0]
    expect(block?.type).toBe("tool")
    if (!block || block.type !== "tool") {
      throw new Error("expected first block to hydrate as tool block")
    }

    expect(block.tool.output).toBe('<div class="card">live</div>warn: noisy')
    expect(block.tool.outputSegments).toEqual([
      { stream: "stdout", text: '<div class="card">live</div>' },
      { stream: "stderr", text: "warn: noisy" },
    ])
  })

  test("repairs placeholder timestamps when output arrives before tool start", () => {
    const withOutputFirst = applyStreamEventToBlocks([], {
      kind: "tool_output_delta",
      invocation_id: "widget-early-1",
      stream: "stdout",
      text: '<div class="card">live</div>',
      session_id: "session-1",
      turn_id: "turn-1",
    })

    const blocks = applyStreamEventToBlocks(withOutputFirst, {
      kind: "tool_call_started",
      invocation_id: "widget-early-1",
      tool_name: "WidgetRenderer",
      arguments: {
        title: "流式 widget",
        description: "先输出后开始",
      },
      started_at_ms: 220,
      session_id: "session-1",
      turn_id: "turn-1",
    })

    const block = blocks[0]
    expect(block?.type).toBe("tool")
    if (!block || block.type !== "tool") {
      throw new Error("expected repaired placeholder tool block")
    }

    expect(block.tool.toolName).toBe("WidgetRenderer")
    expect(block.tool.arguments).toEqual({
      title: "流式 widget",
      description: "先输出后开始",
    })
    expect(block.tool.detectedAtMs).toBe(220)
    expect(block.tool.startedAtMs).toBe(220)
  })

  test("continues merging hydrated widget segments with later deltas", () => {
    const streaming = currentTurnToStreamingTurn({
      started_at_ms: 1,
      user_messages: ["渲染 widget"],
      status: "working",
      blocks: [
        {
          kind: "tool",
          tool: {
            invocation_id: "widget-merge-1",
            tool_name: "WidgetRenderer",
            arguments: {
              title: "流式 widget",
              description: "恢复后继续流式",
            },
            detected_at_ms: 1,
            started_at_ms: 1,
            finished_at_ms: null,
            output: '<div class="card">live',
            output_segments: [
              { stream: "stdout", text: '<div class="card">live' },
            ],
            completed: false,
            result_content: null,
            result_details: null,
            failed: null,
          },
        },
      ],
    })

    const blocks = applyStreamEventToBlocks(streaming.blocks, {
      kind: "tool_output_delta",
      invocation_id: "widget-merge-1",
      stream: "stdout",
      text: "</div>",
      session_id: "session-1",
      turn_id: "turn-1",
    })

    const block = blocks[0]
    expect(block?.type).toBe("tool")
    if (!block || block.type !== "tool") {
      throw new Error("expected hydrated tool block to keep merging")
    }

    expect(block.tool.output).toBe('<div class="card">live</div>')
    expect(block.tool.outputSegments).toEqual([
      { stream: "stdout", text: '<div class="card">live</div>' },
    ])
    expect(block.tool.previewHtml).toBe('<div class="card">live</div>')
  })

  test("hydrates current turn raw widget arguments for parameter streaming", () => {
    const streaming = currentTurnToStreamingTurn({
      started_at_ms: 1,
      user_messages: ["渲染 widget"],
      status: "working",
      blocks: [
        {
          kind: "tool",
          tool: {
            invocation_id: "widget-raw-1",
            tool_name: "WidgetRenderer",
            arguments: {
              title: "流式 widget",
              description: "恢复参数流",
            },
            widget: {
              instance_id: "widget-raw-1",
              phase: "preview",
              document: {
                title: "流式 widget",
                description: "恢复参数流",
                html: '<div class="card">li',
                content_type: "text/html",
              },
            },
            raw_arguments:
              '{"title":"流式 widget","description":"恢复参数流","html":"<div class=\\"card\\">li',
            detected_at_ms: 1,
            started_at_ms: null,
            finished_at_ms: null,
            output: "",
            output_segments: null,
            completed: false,
            result_content: null,
            result_details: null,
            failed: null,
          },
        },
      ],
    })

    const block = streaming.blocks[0]
    expect(block?.type).toBe("tool")
    if (!block || block.type !== "tool") {
      throw new Error("expected hydrated tool block")
    }

    expect(block.tool.rawArguments).toBe(
      '{"title":"流式 widget","description":"恢复参数流","html":"<div class=\\"card\\">li'
    )
    expect(block.tool.widget).toEqual({
      instanceId: "widget-raw-1",
      phase: "preview",
      document: {
        title: "流式 widget",
        description: "恢复参数流",
        html: '<div class="card">li',
        contentType: "text/html",
      },
    })
    expect(block.tool.previewHtml).toBe('<div class="card">li')
  })
})
