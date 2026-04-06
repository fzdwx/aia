export type WidgetPhase = "preview" | "final"

export type UiWidgetDocument = {
  title: string
  description: string
  html: string
  content_type: string
}

export type UiWidget = {
  instance_id: string
  phase: WidgetPhase
  document: UiWidgetDocument
}

export type WidgetColorScheme = "light" | "dark"

export type WidgetCanvasSnapshot = {
  key: string
  data_url: string
}

export type WidgetHostCommand =
  | {
      type: "render"
      widget: UiWidget
    }
  | {
      type: "theme_tokens"
      tokens: Record<string, string>
      color_scheme?: WidgetColorScheme
    }

export type WidgetClientEvent =
  | { type: "ready" }
  | { type: "scripts_ready" }
  | { type: "resize"; height: number; first: boolean }
  | { type: "error"; message: string }
  | { type: "send_prompt"; text: string }
  | { type: "open_link"; href: string }
  | {
      type: "captured"
      html?: string
      styles?: string
      canvases: WidgetCanvasSnapshot[]
      body_width: number
      body_height: number
    }

type WidgetRenderLegacyPayload = {
  type: "aia-widget-update" | "aia-widget-finalize"
  title: string
  description: string | null
  html: string
}

type WidgetThemeLegacyPayload = {
  type: "aia-widget-theme"
  tokens: Record<string, string>
  colorScheme: WidgetColorScheme
}

type LegacyWidgetReadyEvent = {
  type: "aia-widget-ready"
}

type LegacyWidgetHeightEvent = {
  type: "aia-widget-height"
  height: number
}

type LegacyWidgetErrorEvent = {
  type: "aia-widget-error"
  message: string
}

type LegacyWidgetSendPromptEvent = {
  type: "aia-widget-send-prompt"
  text: string
}

type LegacyWidgetOpenLinkEvent = {
  type: "aia-widget-open-link"
  url: string
}

export type WidgetRenderPayloads = {
  shared: Extract<WidgetHostCommand, { type: "render" }>
  legacy: WidgetRenderLegacyPayload
}

export type WidgetThemePayloads = {
  shared: Extract<WidgetHostCommand, { type: "theme_tokens" }>
  legacy: WidgetThemeLegacyPayload
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null
}

function isStringRecord(value: unknown): value is Record<string, string> {
  if (!isRecord(value)) {
    return false
  }

  return Object.values(value).every((entry) => typeof entry === "string")
}

function normalizeCanvases(value: unknown): WidgetCanvasSnapshot[] | null {
  if (!Array.isArray(value)) {
    return null
  }

  const canvases: WidgetCanvasSnapshot[] = []
  for (const item of value) {
    if (!isRecord(item)) {
      return null
    }

    if (typeof item.key !== "string" || typeof item.data_url !== "string") {
      return null
    }

    canvases.push({ key: item.key, data_url: item.data_url })
  }

  return canvases
}

export function createWidgetRenderPayloads(
  widget: UiWidget
): WidgetRenderPayloads {
  return {
    shared: {
      type: "render",
      widget,
    },
    legacy: {
      type:
        widget.phase === "final" ? "aia-widget-finalize" : "aia-widget-update",
      title: widget.document.title,
      description: widget.document.description || null,
      html: widget.document.html,
    },
  }
}

export function createWidgetThemePayloads(input: {
  tokens: Record<string, string>
  colorScheme: WidgetColorScheme
}): WidgetThemePayloads {
  return {
    shared: {
      type: "theme_tokens",
      tokens: input.tokens,
      color_scheme: input.colorScheme,
    },
    legacy: {
      type: "aia-widget-theme",
      tokens: input.tokens,
      colorScheme: input.colorScheme,
    },
  }
}

export function normalizeWidgetClientEvent(
  value: unknown
): WidgetClientEvent | null {
  if (!isRecord(value) || typeof value.type !== "string") {
    return null
  }

  switch (value.type) {
    case "ready":
      return { type: "ready" }
    case "aia-widget-ready": {
      const payload = value as LegacyWidgetReadyEvent
      return { type: payload.type === "aia-widget-ready" ? "ready" : "ready" }
    }
    case "scripts_ready":
      return { type: "scripts_ready" }
    case "resize":
      if (typeof value.height !== "number" || !Number.isFinite(value.height)) {
        return null
      }
      return {
        type: "resize",
        height: value.height,
        first: value.first === true,
      }
    case "aia-widget-height": {
      const payload = value as LegacyWidgetHeightEvent
      if (!Number.isFinite(payload.height)) {
        return null
      }
      return { type: "resize", height: payload.height, first: false }
    }
    case "error":
      if (
        typeof value.message !== "string" ||
        value.message.trim().length === 0
      ) {
        return null
      }
      return { type: "error", message: value.message }
    case "aia-widget-error": {
      const payload = value as LegacyWidgetErrorEvent
      if (payload.message.trim().length === 0) {
        return null
      }
      return { type: "error", message: payload.message }
    }
    case "send_prompt":
      if (typeof value.text !== "string" || value.text.trim().length === 0) {
        return null
      }
      return { type: "send_prompt", text: value.text }
    case "aia-widget-send-prompt": {
      const payload = value as LegacyWidgetSendPromptEvent
      if (payload.text.trim().length === 0) {
        return null
      }
      return { type: "send_prompt", text: payload.text }
    }
    case "open_link":
      if (typeof value.href !== "string" || value.href.trim().length === 0) {
        return null
      }
      return { type: "open_link", href: value.href }
    case "aia-widget-open-link": {
      const payload = value as LegacyWidgetOpenLinkEvent
      if (payload.url.trim().length === 0) {
        return null
      }
      return { type: "open_link", href: payload.url }
    }
    case "captured": {
      const canvases = normalizeCanvases(value.canvases)
      if (
        canvases == null ||
        typeof value.body_width !== "number" ||
        !Number.isFinite(value.body_width) ||
        typeof value.body_height !== "number" ||
        !Number.isFinite(value.body_height)
      ) {
        return null
      }

      const event: WidgetClientEvent = {
        type: "captured",
        canvases,
        body_width: value.body_width,
        body_height: value.body_height,
      }

      if (typeof value.html === "string") {
        event.html = value.html
      }
      if (typeof value.styles === "string") {
        event.styles = value.styles
      }

      return event
    }
    default:
      return null
  }
}

export function isWidgetHostCommand(
  value: unknown
): value is WidgetHostCommand {
  if (!isRecord(value) || typeof value.type !== "string") {
    return false
  }

  if (value.type === "render") {
    return (
      isRecord(value.widget) &&
      typeof value.widget.instance_id === "string" &&
      (value.widget.phase === "preview" || value.widget.phase === "final") &&
      isRecord(value.widget.document) &&
      typeof value.widget.document.title === "string" &&
      typeof value.widget.document.description === "string" &&
      typeof value.widget.document.html === "string" &&
      typeof value.widget.document.content_type === "string"
    )
  }

  if (value.type === "theme_tokens") {
    return isStringRecord(value.tokens)
  }

  return false
}
