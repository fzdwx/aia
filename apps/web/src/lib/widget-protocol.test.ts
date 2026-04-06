import { describe, expect, test } from "vite-plus/test"

import {
  createWidgetRenderPayloads,
  createWidgetThemePayloads,
  isWidgetHostCommand,
  normalizeWidgetClientEvent,
  type UiWidget,
} from "./widget-protocol"

function createWidget(phase: "preview" | "final"): UiWidget {
  return {
    instance_id: "widget-1",
    phase,
    document: {
      title: "粒子黑洞",
      description: "一个可交互的粒子黑洞 widget。",
      html: '<div class="card">demo</div>',
      content_type: "text/html",
    },
  }
}

describe("widget protocol", () => {
  test("creates canonical render command with legacy compatibility payload", () => {
    const payloads = createWidgetRenderPayloads(createWidget("preview"))

    expect(payloads.shared).toEqual({
      type: "render",
      widget: createWidget("preview"),
    })
    expect(payloads.legacy).toEqual({
      type: "aia-widget-update",
      title: "粒子黑洞",
      description: "一个可交互的粒子黑洞 widget。",
      html: '<div class="card">demo</div>',
    })
    expect(isWidgetHostCommand(payloads.shared)).toBe(true)
  })

  test("creates canonical theme command with legacy compatibility payload", () => {
    const payloads = createWidgetThemePayloads({
      colorScheme: "dark",
      tokens: {
        "--foreground": "#fff",
      },
    })

    expect(payloads.shared).toEqual({
      type: "theme_tokens",
      color_scheme: "dark",
      tokens: {
        "--foreground": "#fff",
      },
    })
    expect(payloads.legacy).toEqual({
      type: "aia-widget-theme",
      colorScheme: "dark",
      tokens: {
        "--foreground": "#fff",
      },
    })
    expect(isWidgetHostCommand(payloads.shared)).toBe(true)
  })

  test("normalizes shared and legacy widget client events", () => {
    expect(normalizeWidgetClientEvent({ type: "ready" })).toEqual({
      type: "ready",
    })
    expect(normalizeWidgetClientEvent({ type: "aia-widget-ready" })).toEqual({
      type: "ready",
    })
    expect(normalizeWidgetClientEvent({ type: "scripts_ready" })).toEqual({
      type: "scripts_ready",
    })
    expect(
      normalizeWidgetClientEvent({ type: "resize", height: 320, first: true })
    ).toEqual({ type: "resize", height: 320, first: true })
    expect(
      normalizeWidgetClientEvent({ type: "aia-widget-height", height: 240 })
    ).toEqual({ type: "resize", height: 240, first: false })
    expect(
      normalizeWidgetClientEvent({ type: "send_prompt", text: "继续" })
    ).toEqual({ type: "send_prompt", text: "继续" })
    expect(
      normalizeWidgetClientEvent({
        type: "aia-widget-send-prompt",
        text: "继续",
      })
    ).toEqual({ type: "send_prompt", text: "继续" })
    expect(
      normalizeWidgetClientEvent({ type: "open_link", href: "https://aia.dev" })
    ).toEqual({ type: "open_link", href: "https://aia.dev" })
    expect(
      normalizeWidgetClientEvent({
        type: "aia-widget-open-link",
        url: "https://aia.dev",
      })
    ).toEqual({ type: "open_link", href: "https://aia.dev" })
    expect(
      normalizeWidgetClientEvent({ type: "error", message: "widget failed" })
    ).toEqual({ type: "error", message: "widget failed" })
    expect(
      normalizeWidgetClientEvent({
        type: "aia-widget-error",
        message: "widget failed",
      })
    ).toEqual({ type: "error", message: "widget failed" })
  })

  test("normalizes captured events and rejects invalid payloads", () => {
    expect(
      normalizeWidgetClientEvent({
        type: "captured",
        html: "<div>demo</div>",
        styles: ".card { color: red; }",
        canvases: [{ key: "main", data_url: "data:image/png;base64,abc" }],
        body_width: 640,
        body_height: 480,
      })
    ).toEqual({
      type: "captured",
      html: "<div>demo</div>",
      styles: ".card { color: red; }",
      canvases: [{ key: "main", data_url: "data:image/png;base64,abc" }],
      body_width: 640,
      body_height: 480,
    })

    expect(normalizeWidgetClientEvent({ type: "resize", height: "bad" })).toBe(
      null
    )
    expect(normalizeWidgetClientEvent({ type: "open_link", href: "" })).toBe(
      null
    )
    expect(
      isWidgetHostCommand({ type: "theme_tokens", tokens: { ok: 1 } })
    ).toBe(false)
  })
})
