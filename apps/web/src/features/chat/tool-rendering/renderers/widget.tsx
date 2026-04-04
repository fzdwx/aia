import { useEffect, useMemo, useRef } from "react"

import { normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import {
  getArrayValue,
  getStringValue,
  truncateInline,
} from "../helpers"
import {
  ExpandableOutput,
  ToolDetailSection,
  ToolDetailSurface,
  ToolInfoSection,
} from "../ui"

type WidgetSandboxProps = {
  title: string
  description: string | null
  html: string
}

const WIDGET_IFRAME_BASE_CSS = `
  :root {
    color-scheme: light dark;
    font-family: var(--font-sans, ui-sans-serif, system-ui, sans-serif);
  }
  html, body {
    margin: 0;
    padding: 0;
    background: transparent;
    color: var(--foreground, CanvasText);
    font: 14px/1.6 var(--font-sans, ui-sans-serif, system-ui, sans-serif);
  }
  body {
    min-height: 1px;
    overflow: hidden;
  }
  h1, h2, h3, h4, h5, h6, p {
    margin: 0;
  }
  .card {
    background: var(--card, transparent);
    color: var(--card-foreground, var(--foreground, CanvasText));
    border: 0.5px solid var(--border, currentColor);
    border-radius: calc(var(--radius, 12px) * 1.5);
    padding: 1rem 1.25rem;
    box-sizing: border-box;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 0.375rem;
    border-radius: 999px;
    border: 0.5px solid var(--border, currentColor);
    padding: 0.2rem 0.55rem;
    background: transparent;
    box-sizing: border-box;
  }
  .badge.primary {
    background: var(--primary, currentColor);
    color: var(--primary-foreground, Canvas);
    border-color: transparent;
  }
  button.primary {
    background: var(--primary, currentColor);
    color: var(--primary-foreground, Canvas);
    border-color: transparent;
  }
  button.destructive {
    color: var(--destructive, var(--foreground, CanvasText));
  }
  button, input, select, textarea, label, code, pre, table, th, td, a {
    font: inherit;
  }
  button {
    border: 0.5px solid var(--border, currentColor);
    border-radius: var(--radius, 12px);
    background: transparent;
    color: var(--foreground, CanvasText);
    padding: 0.5rem 0.8rem;
    cursor: pointer;
    transition: background-color .15s ease, border-color .15s ease;
    box-sizing: border-box;
  }
  button:hover {
    background: var(--muted, transparent);
  }
  input[type="text"],
  input[type="number"],
  input[type="search"],
  textarea,
  select {
    width: 100%;
    min-height: 36px;
    border: 0.5px solid var(--border, currentColor);
    border-radius: var(--radius, 12px);
    background: var(--background, transparent);
    color: var(--foreground, CanvasText);
    padding: 0.5rem 0.75rem;
    box-sizing: border-box;
  }
  input[type="range"] {
    width: 100%;
    accent-color: var(--primary, currentColor);
  }
  a {
    color: var(--primary, currentColor);
    text-decoration: none;
  }
  a:hover {
    text-decoration: underline;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th, td {
    text-align: left;
    padding: 0.45rem 0.5rem;
    border-bottom: 0.5px solid var(--border, currentColor);
  }
  code, pre {
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    border: 0;
  }
`

const WIDGET_IFRAME_SVG_CSS = `
  svg {
    display: block;
    width: 100%;
    height: auto;
    overflow: visible;
  }
  svg .t,
  svg .th,
  svg .ts {
    font-family: var(--font-sans, ui-sans-serif, system-ui, sans-serif);
    dominant-baseline: middle;
  }
  svg .t {
    fill: var(--foreground, CanvasText);
    font-size: 14px;
    font-weight: 400;
  }
  svg .th {
    fill: var(--foreground, CanvasText);
    font-size: 14px;
    font-weight: 500;
  }
  svg .ts {
    fill: var(--muted-foreground, CanvasText);
    font-size: 12px;
    font-weight: 400;
  }
  svg .box {
    fill: var(--muted, transparent);
    stroke: var(--border, currentColor);
    stroke-width: 0.5;
  }
  svg .arr {
    fill: none;
    stroke: var(--border, currentColor);
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
    marker-end: url(#arrow);
  }
  svg .leader {
    fill: none;
    stroke: var(--border, currentColor);
    stroke-opacity: 0.7;
    stroke-width: 0.5;
    stroke-dasharray: 4 4;
  }
  svg .node {
    cursor: pointer;
    transition: opacity .15s ease;
  }
  svg .node:hover {
    opacity: 0.88;
  }
  svg {
    --p: var(--primary);
    --s: var(--secondary);
    --t: var(--foreground);
    --bg2: var(--muted);
    --b: var(--border);
  }
  svg .c-purple > rect,
  svg .c-purple > circle,
  svg .c-purple > ellipse,
  svg rect.c-purple,
  svg circle.c-purple,
  svg ellipse.c-purple {
    fill: var(--ramp-purple-fill);
    stroke: var(--ramp-purple-stroke);
  }
  svg .c-purple > .t,
  svg .c-purple > .th,
  svg .c-purple > .ts,
  svg text.c-purple {
    fill: var(--ramp-purple-text);
  }
  svg .c-teal > rect,
  svg .c-teal > circle,
  svg .c-teal > ellipse,
  svg rect.c-teal,
  svg circle.c-teal,
  svg ellipse.c-teal {
    fill: var(--ramp-teal-fill);
    stroke: var(--ramp-teal-stroke);
  }
  svg .c-teal > .t,
  svg .c-teal > .th,
  svg .c-teal > .ts,
  svg text.c-teal {
    fill: var(--ramp-teal-text);
  }
  svg .c-coral > rect,
  svg .c-coral > circle,
  svg .c-coral > ellipse,
  svg rect.c-coral,
  svg circle.c-coral,
  svg ellipse.c-coral {
    fill: var(--ramp-coral-fill);
    stroke: var(--ramp-coral-stroke);
  }
  svg .c-coral > .t,
  svg .c-coral > .th,
  svg .c-coral > .ts,
  svg text.c-coral {
    fill: var(--ramp-coral-text);
  }
  svg .c-pink > rect,
  svg .c-pink > circle,
  svg .c-pink > ellipse,
  svg rect.c-pink,
  svg circle.c-pink,
  svg ellipse.c-pink {
    fill: var(--ramp-pink-fill);
    stroke: var(--ramp-pink-stroke);
  }
  svg .c-pink > .t,
  svg .c-pink > .th,
  svg .c-pink > .ts,
  svg text.c-pink {
    fill: var(--ramp-pink-text);
  }
  svg .c-gray > rect,
  svg .c-gray > circle,
  svg .c-gray > ellipse,
  svg rect.c-gray,
  svg circle.c-gray,
  svg ellipse.c-gray {
    fill: var(--ramp-gray-fill);
    stroke: var(--ramp-gray-stroke);
  }
  svg .c-gray > .t,
  svg .c-gray > .th,
  svg .c-gray > .ts,
  svg text.c-gray {
    fill: var(--ramp-gray-text);
  }
  svg .c-blue > rect,
  svg .c-blue > circle,
  svg .c-blue > ellipse,
  svg rect.c-blue,
  svg circle.c-blue,
  svg ellipse.c-blue {
    fill: var(--ramp-blue-fill);
    stroke: var(--ramp-blue-stroke);
  }
  svg .c-blue > .t,
  svg .c-blue > .th,
  svg .c-blue > .ts,
  svg text.c-blue {
    fill: var(--ramp-blue-text);
  }
  svg .c-green > rect,
  svg .c-green > circle,
  svg .c-green > ellipse,
  svg rect.c-green,
  svg circle.c-green,
  svg ellipse.c-green {
    fill: var(--ramp-green-fill);
    stroke: var(--ramp-green-stroke);
  }
  svg .c-green > .t,
  svg .c-green > .th,
  svg .c-green > .ts,
  svg text.c-green {
    fill: var(--ramp-green-text);
  }
  svg .c-amber > rect,
  svg .c-amber > circle,
  svg .c-amber > ellipse,
  svg rect.c-amber,
  svg circle.c-amber,
  svg ellipse.c-amber {
    fill: var(--ramp-amber-fill);
    stroke: var(--ramp-amber-stroke);
  }
  svg .c-amber > .t,
  svg .c-amber > .th,
  svg .c-amber > .ts,
  svg text.c-amber {
    fill: var(--ramp-amber-text);
  }
  svg .c-red > rect,
  svg .c-red > circle,
  svg .c-red > ellipse,
  svg rect.c-red,
  svg circle.c-red,
  svg ellipse.c-red {
    fill: var(--ramp-red-fill);
    stroke: var(--ramp-red-stroke);
  }
  svg .c-red > .t,
  svg .c-red > .th,
  svg .c-red > .ts,
  svg text.c-red {
    fill: var(--ramp-red-text);
  }
`

function getStreamingWidgetHtml(data: {
  outputContent: string
  outputSegments?: { stream: "stdout" | "stderr"; text: string }[]
}) {
  const stdout = (data.outputSegments ?? [])
    .filter((segment) => segment.stream === "stdout")
    .map((segment) => segment.text)
    .join("")
    .trim()

  if (stdout.length > 0) return stdout

  const output = data.outputContent.trim()
  return output.length > 0 ? output : ""
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;")
}

function buildThemeTokenScript(): string {
  return `
    const THEME_ATTRS = [
      'style',
      'class',
      'data-theme'
    ];

    const THEME_KEYS = [
      '--font-sans',
      '--font-serif',
      '--font-mono',
      '--radius',
      '--background',
      '--foreground',
      '--card',
      '--card-foreground',
      '--popover',
      '--popover-foreground',
      '--primary',
      '--primary-foreground',
      '--secondary',
      '--secondary-foreground',
      '--muted',
      '--muted-foreground',
      '--accent',
      '--accent-foreground',
      '--destructive',
      '--destructive-foreground',
      '--border',
      '--input',
      '--ring'
    ];

    const RAMP_VALUES = {
      purple: { lightFill: '#EEEDFE', lightStroke: '#534AB7', lightText: '#3C3489', darkFill: '#3C3489', darkStroke: '#AFA9EC', darkText: '#CECBF6' },
      teal: { lightFill: '#E1F5EE', lightStroke: '#0F6E56', lightText: '#085041', darkFill: '#085041', darkStroke: '#5DCAA5', darkText: '#9FE1CB' },
      coral: { lightFill: '#FAECE7', lightStroke: '#993C1D', lightText: '#712B13', darkFill: '#712B13', darkStroke: '#F0997B', darkText: '#F5C4B3' },
      pink: { lightFill: '#FBEAF0', lightStroke: '#993556', lightText: '#72243E', darkFill: '#72243E', darkStroke: '#ED93B1', darkText: '#F4C0D1' },
      gray: { lightFill: '#F1EFE8', lightStroke: '#5F5E5A', lightText: '#444441', darkFill: '#444441', darkStroke: '#B4B2A9', darkText: '#D3D1C7' },
      blue: { lightFill: '#E6F1FB', lightStroke: '#185FA5', lightText: '#0C447C', darkFill: '#0C447C', darkStroke: '#85B7EB', darkText: '#B5D4F4' },
      green: { lightFill: '#EAF3DE', lightStroke: '#3B6D11', lightText: '#27500A', darkFill: '#27500A', darkStroke: '#97C459', darkText: '#C0DD97' },
      amber: { lightFill: '#FAEEDA', lightStroke: '#854F0B', lightText: '#633806', darkFill: '#633806', darkStroke: '#EF9F27', darkText: '#FAC775' },
      red: { lightFill: '#FCEBEB', lightStroke: '#A32D2D', lightText: '#791F1F', darkFill: '#791F1F', darkStroke: '#F09595', darkText: '#F7C1C1' },
    };

    const hostRoot = parent.document.documentElement;

    const applyHostTheme = () => {
      const computed = parent.getComputedStyle(hostRoot);
      for (const key of THEME_KEYS) {
        const value = computed.getPropertyValue(key);
        if (value) {
          document.documentElement.style.setProperty(key, value.trim());
        }
      }

      const colorScheme = hostRoot.classList.contains('light') ? 'light' : 'dark';
      document.documentElement.style.colorScheme = colorScheme;
      document.documentElement.classList.toggle('light', colorScheme === 'light');
      document.documentElement.classList.toggle('dark', colorScheme === 'dark');

      const isLight = colorScheme === 'light';
      for (const [name, ramp] of Object.entries(RAMP_VALUES)) {
        document.documentElement.style.setProperty(
          '--ramp-' + name + '-fill',
          isLight ? ramp.lightFill : ramp.darkFill
        );
        document.documentElement.style.setProperty(
          '--ramp-' + name + '-stroke',
          isLight ? ramp.lightStroke : ramp.darkStroke
        );
        document.documentElement.style.setProperty(
          '--ramp-' + name + '-text',
          isLight ? ramp.lightText : ramp.darkText
        );
      }
    };

    applyHostTheme();

    const hostObserver = new MutationObserver(() => {
      applyHostTheme();
      postHeight();
    });

    hostObserver.observe(hostRoot, {
      attributes: true,
      attributeFilter: THEME_ATTRS,
    });
  `
}

function buildSandboxDocument({ title, description, html }: WidgetSandboxProps): string {
  const safeTitle = escapeHtml(title)
  const safeDescription = description ? escapeHtml(description) : ""
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>${safeTitle}</title>
  <style>
    ${WIDGET_IFRAME_BASE_CSS}
    ${WIDGET_IFRAME_SVG_CSS}
  </style>
</head>
<body>
  ${safeDescription ? `<div class="sr-only">${safeDescription}</div>` : ""}
  ${html}
  <script>
    const postHeight = () => {
      const nextHeight = Math.max(
        document.documentElement.scrollHeight,
        document.body.scrollHeight,
        1
      );
      parent.postMessage({ type: 'aia-widget-height', height: nextHeight }, '*');
    };

    window.sendPrompt = (text) => {
      parent.postMessage({ type: 'aia-widget-send-prompt', text }, '*');
    };

    window.openLink = (url) => {
      parent.postMessage({ type: 'aia-widget-open-link', url }, '*');
    };

    ${buildThemeTokenScript()}

    document.addEventListener('click', (event) => {
      const link = event.target instanceof Element ? event.target.closest('a[href]') : null;
      if (!link) return;
      const href = link.getAttribute('href');
      if (!href) return;
      event.preventDefault();
      window.openLink(href);
    });

    window.addEventListener('error', (event) => {
      parent.postMessage({
        type: 'aia-widget-error',
        message: event.message || 'Widget runtime error'
      }, '*');
    });

    window.addEventListener('unhandledrejection', (event) => {
      const reason = event.reason;
      const message = typeof reason === 'string'
        ? reason
        : reason && typeof reason.message === 'string'
          ? reason.message
          : 'Widget promise rejection';
      parent.postMessage({ type: 'aia-widget-error', message }, '*');
    });

    const observer = new ResizeObserver(postHeight);
    observer.observe(document.documentElement);
    observer.observe(document.body);
    window.addEventListener('load', postHeight);
    requestAnimationFrame(postHeight);
    setTimeout(postHeight, 0);
  </script>
</body>
</html>`
}

function WidgetSandbox({ title, description, html }: WidgetSandboxProps) {
  const iframeRef = useRef<HTMLIFrameElement | null>(null)
  const srcDoc = useMemo(
    () => buildSandboxDocument({ title, description, html }),
    [description, html, title]
  )

  useEffect(() => {
    function handleMessage(event: MessageEvent) {
      const frame = iframeRef.current
      if (!frame || event.source !== frame.contentWindow) return

      const payload = event.data as
        | { type?: string; height?: number; text?: string; url?: string; message?: string }
        | null
      if (!payload || typeof payload !== "object") return

      if (payload.type === "aia-widget-height") {
        const nextHeight = Math.max(120, Math.min(1600, payload.height ?? 0))
        frame.style.height = `${nextHeight}px`
        return
      }

      if (
        payload.type === "aia-widget-send-prompt" &&
        typeof payload.text === "string" &&
        payload.text.trim().length > 0
      ) {
        void import("@/stores/chat-store").then(({ useChatStore }) => {
          void useChatStore.getState().sendMessage(payload.text!.trim())
        })
        return
      }

      if (
        payload.type === "aia-widget-open-link" &&
        typeof payload.url === "string" &&
        payload.url.trim().length > 0
      ) {
        window.open(payload.url, "_blank", "noopener,noreferrer")
        return
      }

      if (
        payload.type === "aia-widget-error" &&
        typeof payload.message === "string" &&
        payload.message.trim().length > 0
      ) {
        console.warn("Widget sandbox error:", payload.message)
      }
    }

    window.addEventListener("message", handleMessage)
    return () => window.removeEventListener("message", handleMessage)
  }, [])

  return (
    <iframe
      ref={iframeRef}
      title={title}
      srcDoc={srcDoc}
      sandbox="allow-scripts allow-popups"
      className="w-full overflow-hidden rounded-[calc(var(--radius)*1.25)] border border-border/50 bg-transparent"
      style={{ height: 160, overflow: "hidden" }}
    />
  )
}

function createWidgetSummary(data: {
  arguments: Record<string, unknown>
  details?: Record<string, unknown>
}) {
  const details = data.details
  const title =
    getStringValue(details, "title") ?? getStringValue(data.arguments, "title")
  const description =
    getStringValue(details, "description") ??
    getStringValue(data.arguments, "description")

  return {
    title: title ? truncateInline(title, 80) : "Widget",
    description: description ? truncateInline(description, 100) : null,
  }
}

function createWidgetReadmeSummary(data: { arguments: Record<string, unknown> }) {
  const modules = getArrayValue(data.arguments, "modules").flatMap((module) =>
    typeof module === "string" ? [module] : []
  )
  if (modules.length === 0) {
    return "Widget renderer guide"
  }
  return `Widget guide · ${modules.join(", ")}`
}

export function createWidgetReadmeRenderer(): ToolRenderer {
  return {
    matches: (toolName) =>
      toolName === "WidgetReadme" || toolName === "widgetReadme",
    detailsPanelMode: "renderer-only",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      return createWidgetReadmeSummary({ arguments: args })
    },
    renderSubtitle(data) {
      const modules = getArrayValue(normalizeToolArguments(data.arguments), "modules")
        .flatMap((module) => (typeof module === "string" ? [module] : []))
      return modules.length > 0 ? `${modules.length} module(s)` : "overview"
    },
    renderMeta() {
      return null
    },
    renderDetails(data) {
      return (
        <ToolDetailSection title="Content">
          <ExpandableOutput value={data.outputContent} failed={false} />
        </ToolDetailSection>
      )
    },
  }
}

export function createWidgetRendererRenderer(): ToolRenderer {
  return {
    matches: (toolName) =>
      toolName === "WidgetRenderer" || toolName === "widgetRenderer",
    detailsPanelMode: "renderer-only-flat",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      return createWidgetSummary({ arguments: args, details: data.details }).title
    },
    renderSubtitle(data) {
      const args = normalizeToolArguments(data.arguments)
      return createWidgetSummary({ arguments: args, details: data.details })
        .description
    },
    renderMeta() {
      return null
    },
    renderDetails(data) {
      const args = normalizeToolArguments(data.arguments)
      const details = data.details ?? undefined
      const title =
        getStringValue(details, "title") ?? getStringValue(args, "title") ?? "Widget"
      const description =
        getStringValue(details, "description") ??
        getStringValue(args, "description") ??
        null
      const liveHtml = getStreamingWidgetHtml({
        outputContent: data.outputContent,
        outputSegments: data.outputSegments,
      })
      const html = data.isRunning
        ? liveHtml || getStringValue(args, "html") || ""
        : getStringValue(details, "html") || liveHtml || getStringValue(args, "html") || ""

      if (!html.trim()) {
        return (
          <ToolDetailSection title="Content">
            <ExpandableOutput value={data.outputContent} failed={!data.succeeded} />
          </ToolDetailSection>
        )
      }

      return (
        <div className="space-y-3">
          <ToolDetailSurface className="tool-timeline-detail-surface-flat tool-timeline-detail-surface-borderless">
            <WidgetSandbox title={title} description={description} html={html} />
          </ToolDetailSurface>
          <ToolInfoSection title="Content" defaultOpen={false}>
            <ExpandableOutput value={html} failed={false} />
          </ToolInfoSection>
        </div>
      )
    },
  }
}
