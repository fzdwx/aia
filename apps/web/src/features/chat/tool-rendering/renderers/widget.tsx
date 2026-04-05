import { useCallback, useEffect, useRef, useState } from "react"

import { normalizeToolArguments } from "@/lib/tool-display"

import type { ToolRenderer } from "../types"
import { getArrayValue, getStringValue, truncateInline } from "../helpers"
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
  isStreaming: boolean
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
  previewHtml?: string
  outputContent: string
  outputSegments?: { stream: "stdout" | "stderr"; text: string }[]
  rawArguments?: string
}) {
  if (
    typeof data.previewHtml === "string" &&
    data.previewHtml.trim().length > 0
  ) {
    return data.previewHtml.trim()
  }

  const stdout = (data.outputSegments ?? [])
    .filter((segment) => segment.stream === "stdout")
    .map((segment) => segment.text)
    .join("")
    .trim()

  if (stdout.length > 0) return stdout

  const output = data.outputContent.trim()
  if (output.length > 0) return output

  const rawArguments = data.rawArguments ?? ""
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
    const root = document.getElementById('aia-widget-root');
    const descriptionNode = document.getElementById('aia-widget-description');
    let lastRenderedHtml = root ? root.innerHTML : '';
    const DANGEROUS_TAGS = /<(iframe|object|embed|meta|link|base|form)[\\s>][\\s\\S]*?<\\/\\1>/gi;
    const DANGEROUS_VOID = /<(iframe|object|embed|meta|link|base)\\b[^>]*\\/?>/gi;

    const trimIncompleteScripts = (value) => {
      if (typeof value !== 'string' || value.length === 0) {
        return '';
      }

      const openIndex = value.lastIndexOf('<script');
      if (openIndex < 0) {
        return value;
      }

      const closeIndex = value.indexOf('<\\/script>', openIndex);
      return closeIndex < 0 ? value.slice(0, openIndex) : value;
    };

    const stripDangerousUrls = (value) => value.replace(
      /\\s+(href|src|action)\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s>"']*))/gi,
      (match, _attr, dq, sq, uq) => {
        const url = (dq ?? sq ?? uq ?? '').trim();
        return /^\\s*(javascript|data)\\s*:/i.test(url) ? '' : match;
      }
    );

    const sanitizeForStreaming = (value) => stripDangerousUrls(
      trimIncompleteScripts(value)
        .replace(DANGEROUS_TAGS, '')
        .replace(DANGEROUS_VOID, '')
        .replace(/\\s+on[a-z]+\\s*=\\s*(?:"[^"]*"|'[^']*'|[^\\s>"']*)/gi, '')
        .replace(/<script[\\s\\S]*?<\\/script>/gi, '')
        .replace(/<script\\b[^>]*\\/?>/gi, '')
    );

    const sanitizeForFinalize = (value) => trimIncompleteScripts(value)
      .replace(DANGEROUS_TAGS, '')
      .replace(DANGEROUS_VOID, '');

    const runWidgetCleanup = () => {
      if (typeof window.__AIA_WIDGET_CLEANUP__ !== 'function') {
        return;
      }

      try {
        window.__AIA_WIDGET_CLEANUP__();
      } catch (error) {
        const message = error && typeof error.message === 'string'
          ? error.message
          : 'Widget cleanup failed';
        parent.postMessage({ type: 'aia-widget-error', message }, '*');
      }

      window.__AIA_WIDGET_CLEANUP__ = undefined;
    };

    const applyStreamingHtml = (html) => {
      if (!root) {
        return;
      }

      const visualHtml = sanitizeForStreaming(html);
      if (visualHtml !== lastRenderedHtml) {
        root.innerHTML = visualHtml;
        lastRenderedHtml = visualHtml;
      }
    };

    const applyFinalHtml = (html) => {
      if (!root) {
        return;
      }

      const tmp = document.createElement('div');
      tmp.innerHTML = sanitizeForFinalize(html);
      const scripts = Array.from(tmp.querySelectorAll('script')).map((script) => ({
        src: script.getAttribute('src') || '',
        text: script.textContent || '',
        attrs: Array.from(script.attributes)
          .filter((attribute) => attribute.name !== 'src' && attribute.name !== 'onload')
          .map((attribute) => ({ name: attribute.name, value: attribute.value })),
      }));
      for (const script of Array.from(tmp.querySelectorAll('script'))) {
        script.remove();
      }

      const visualHtml = tmp.innerHTML;
      runWidgetCleanup();
      if (visualHtml !== lastRenderedHtml) {
        root.innerHTML = visualHtml;
        lastRenderedHtml = visualHtml;
      }

      const externalScripts = scripts.filter((script) => script.src.length > 0);
      const inlineScripts = scripts.filter(
        (script) => script.src.length === 0 && script.text.length > 0
      );

      const appendInlineScripts = () => {
        for (const script of inlineScripts) {
          const nextScript = document.createElement('script');
          for (const attribute of script.attrs) {
            nextScript.setAttribute(attribute.name, attribute.value);
          }
          nextScript.textContent = script.text;
          root.appendChild(nextScript);
        }
        scheduleHeightSync();
      };

      if (externalScripts.length === 0) {
        appendInlineScripts();
        return;
      }

      let pending = externalScripts.length;
      const onExternalSettled = () => {
        pending -= 1;
        if (pending <= 0) {
          appendInlineScripts();
        }
      };

      for (const script of externalScripts) {
        const nextScript = document.createElement('script');
        nextScript.src = script.src;
        nextScript.onload = onExternalSettled;
        nextScript.onerror = onExternalSettled;
        for (const attribute of script.attrs) {
          nextScript.setAttribute(attribute.name, attribute.value);
        }
        root.appendChild(nextScript);
      }
    };

    let heightSyncRaf = 0;
    let heightSyncTimeout = 0;
    const scheduleHeightSync = () => {
      if (heightSyncRaf) {
        return;
      }

      heightSyncRaf = requestAnimationFrame(() => {
        heightSyncRaf = 0;
        postHeight();

        if (heightSyncTimeout) {
          clearTimeout(heightSyncTimeout);
        }
        heightSyncTimeout = window.setTimeout(() => {
          heightSyncTimeout = 0;
          postHeight();
        }, 96);
      });
    };

    const syncContentPayload = (payload) => {
      if (
        !payload ||
        (payload.type !== 'aia-widget-update' &&
          payload.type !== 'aia-widget-finalize')
      ) {
        return;
      }

      if (typeof payload.title === 'string' && payload.title.length > 0) {
        document.title = payload.title;
      }

      if (descriptionNode) {
        descriptionNode.textContent =
          typeof payload.description === 'string' ? payload.description : '';
      }

      if (root && typeof payload.html === 'string') {
        if (payload.type === 'aia-widget-update') {
          applyStreamingHtml(payload.html);
        } else {
          applyFinalHtml(payload.html);
        }
      }

      scheduleHeightSync();
    };

    const applyThemePayload = (payload) => {
      if (!payload || payload.type !== 'aia-widget-theme') {
        return;
      }

      const root = document.documentElement;
      const tokens = payload.tokens && typeof payload.tokens === 'object'
        ? payload.tokens
        : {};

      for (const [key, value] of Object.entries(tokens)) {
        if (typeof value === 'string' && value.length > 0) {
          root.style.setProperty(key, value);
        }
      }

      const colorScheme = payload.colorScheme === 'light' ? 'light' : 'dark';
      root.style.colorScheme = colorScheme;
      root.classList.toggle('light', colorScheme === 'light');
      root.classList.toggle('dark', colorScheme === 'dark');
      postHeight();
    };

    window.addEventListener('message', (event) => {
      if (event.source !== parent) return;
      syncContentPayload(event.data);
      applyThemePayload(event.data);
    });

    parent.postMessage({ type: 'aia-widget-ready' }, '*');
  `
}

function buildSandboxDocument({
  title,
  description,
}: WidgetSandboxProps): string {
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
  <div id="aia-widget-description" class="sr-only">${safeDescription}</div>
  <div id="aia-widget-root"></div>
  <script>
    const postHeight = () => {
      const rootRectHeight = root
        ? Math.ceil(root.getBoundingClientRect().height)
        : 0;
      const rootScrollHeight = root ? root.scrollHeight : 0;
      const rootOffsetHeight = root ? root.offsetHeight : 0;
      const nextHeight = Math.max(
        rootRectHeight,
        rootScrollHeight,
        rootOffsetHeight,
        document.documentElement.scrollHeight,
        document.documentElement.offsetHeight,
        document.body.scrollHeight,
        document.body.offsetHeight,
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
    if (root) {
      observer.observe(root);
    }
    const mutationObserver = new MutationObserver(() => {
      scheduleHeightSync();
    });
    if (root) {
      mutationObserver.observe(root, {
        childList: true,
        subtree: true,
      });
    }
    window.addEventListener('load', postHeight);
    scheduleHeightSync();
  </script>
</body>
</html>`
}

// eslint-disable-next-line react-refresh/only-export-components
function WidgetSandbox({
  title,
  description,
  html,
  isStreaming,
}: WidgetSandboxProps) {
  const iframeRef = useRef<HTMLIFrameElement | null>(null)
  const lastHeightRef = useRef<number | null>(null)
  const frameReadyRef = useRef(false)
  const pendingRenderTimeoutRef = useRef<number | null>(null)
  const latestRenderPayloadRef = useRef({
    type: isStreaming ? "aia-widget-update" : "aia-widget-finalize",
    title,
    description,
    html,
  })
  const [frameHeight, setFrameHeight] = useState<number | undefined>(undefined)
  const initialSrcDocRef = useRef<string>(
    buildSandboxDocument({ title, description, html: "" })
  )
  const srcDoc = initialSrcDocRef.current

  const flushRenderPayload = useCallback(
    (options?: { immediate?: boolean }) => {
      const target = iframeRef.current?.contentWindow
      if (!target || !frameReadyRef.current) {
        return
      }

      if (pendingRenderTimeoutRef.current != null) {
        window.clearTimeout(pendingRenderTimeoutRef.current)
        pendingRenderTimeoutRef.current = null
      }

      const send = () => {
        pendingRenderTimeoutRef.current = null
        target.postMessage(latestRenderPayloadRef.current, "*")
      }

      if (
        options?.immediate ||
        latestRenderPayloadRef.current.type === "aia-widget-finalize"
      ) {
        send()
        return
      }

      pendingRenderTimeoutRef.current = window.setTimeout(send, 48)
    },
    []
  )

  useEffect(() => {
    const THEME_KEYS = [
      "--font-sans",
      "--font-serif",
      "--font-mono",
      "--radius",
      "--background",
      "--foreground",
      "--card",
      "--card-foreground",
      "--popover",
      "--popover-foreground",
      "--primary",
      "--primary-foreground",
      "--secondary",
      "--secondary-foreground",
      "--muted",
      "--muted-foreground",
      "--accent",
      "--accent-foreground",
      "--destructive",
      "--destructive-foreground",
      "--border",
      "--input",
      "--ring",
    ] as const

    const RAMP_VALUES = {
      purple: {
        lightFill: "#EEEDFE",
        lightStroke: "#534AB7",
        lightText: "#3C3489",
        darkFill: "#3C3489",
        darkStroke: "#AFA9EC",
        darkText: "#CECBF6",
      },
      teal: {
        lightFill: "#E1F5EE",
        lightStroke: "#0F6E56",
        lightText: "#085041",
        darkFill: "#085041",
        darkStroke: "#5DCAA5",
        darkText: "#9FE1CB",
      },
      coral: {
        lightFill: "#FAECE7",
        lightStroke: "#993C1D",
        lightText: "#712B13",
        darkFill: "#712B13",
        darkStroke: "#F0997B",
        darkText: "#F5C4B3",
      },
      pink: {
        lightFill: "#FBEAF0",
        lightStroke: "#993556",
        lightText: "#72243E",
        darkFill: "#72243E",
        darkStroke: "#ED93B1",
        darkText: "#F4C0D1",
      },
      gray: {
        lightFill: "#F1EFE8",
        lightStroke: "#5F5E5A",
        lightText: "#444441",
        darkFill: "#444441",
        darkStroke: "#B4B2A9",
        darkText: "#D3D1C7",
      },
      blue: {
        lightFill: "#E6F1FB",
        lightStroke: "#185FA5",
        lightText: "#0C447C",
        darkFill: "#0C447C",
        darkStroke: "#85B7EB",
        darkText: "#B5D4F4",
      },
      green: {
        lightFill: "#EAF3DE",
        lightStroke: "#3B6D11",
        lightText: "#27500A",
        darkFill: "#27500A",
        darkStroke: "#97C459",
        darkText: "#C0DD97",
      },
      amber: {
        lightFill: "#FAEEDA",
        lightStroke: "#854F0B",
        lightText: "#633806",
        darkFill: "#633806",
        darkStroke: "#EF9F27",
        darkText: "#FAC775",
      },
      red: {
        lightFill: "#FCEBEB",
        lightStroke: "#A32D2D",
        lightText: "#791F1F",
        darkFill: "#791F1F",
        darkStroke: "#F09595",
        darkText: "#F7C1C1",
      },
    } as const

    function sendThemeToFrame() {
      const frame = iframeRef.current
      const target = frame?.contentWindow
      if (!target) return

      const hostRoot = document.documentElement
      const computed = window.getComputedStyle(hostRoot)
      const colorScheme = hostRoot.classList.contains("light")
        ? "light"
        : "dark"
      const isLight = colorScheme === "light"

      const tokens: Record<string, string> = {}
      for (const key of THEME_KEYS) {
        const value = computed.getPropertyValue(key).trim()
        if (value) {
          tokens[key] = value
        }
      }

      for (const [name, ramp] of Object.entries(RAMP_VALUES)) {
        tokens[`--ramp-${name}-fill`] = isLight ? ramp.lightFill : ramp.darkFill
        tokens[`--ramp-${name}-stroke`] = isLight
          ? ramp.lightStroke
          : ramp.darkStroke
        tokens[`--ramp-${name}-text`] = isLight ? ramp.lightText : ramp.darkText
      }

      target.postMessage(
        {
          type: "aia-widget-theme",
          colorScheme,
          tokens,
        },
        "*"
      )
    }

    function handleMessage(event: MessageEvent) {
      const frame = iframeRef.current
      if (!frame || event.source !== frame.contentWindow) return

      const payload = event.data as {
        type?: string
        height?: number
        text?: string
        url?: string
        message?: string
      } | null
      if (!payload || typeof payload !== "object") return

      if (payload.type === "aia-widget-ready") {
        frameReadyRef.current = true
        flushRenderPayload({ immediate: true })
        sendThemeToFrame()
        return
      }

      if (payload.type === "aia-widget-height") {
        const reportedHeight = Math.ceil(payload.height ?? 0)
        if (!Number.isFinite(reportedHeight) || reportedHeight <= 0) {
          return
        }

        const nextHeight = Math.max(1, reportedHeight)
        if (
          lastHeightRef.current != null &&
          Math.abs(nextHeight - lastHeightRef.current) < 1
        ) {
          return
        }

        lastHeightRef.current = nextHeight
        frame.style.height = `${nextHeight}px`
        setFrameHeight(nextHeight)
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

    const hostRoot = document.documentElement
    const themeObserver = new MutationObserver(() => {
      sendThemeToFrame()
    })

    window.addEventListener("message", handleMessage)
    themeObserver.observe(hostRoot, {
      attributes: true,
      attributeFilter: ["class", "style", "data-theme"],
    })

    const frame = iframeRef.current
    frameReadyRef.current = false
    frame?.addEventListener("load", sendThemeToFrame)
    sendThemeToFrame()

    return () => {
      if (pendingRenderTimeoutRef.current != null) {
        window.clearTimeout(pendingRenderTimeoutRef.current)
      }
      window.removeEventListener("message", handleMessage)
      themeObserver.disconnect()
      frame?.removeEventListener("load", sendThemeToFrame)
    }
  }, [flushRenderPayload])

  useEffect(() => {
    latestRenderPayloadRef.current = {
      type: isStreaming ? "aia-widget-update" : "aia-widget-finalize",
      title,
      description,
      html,
    }

    const target = iframeRef.current?.contentWindow
    if (target && frameReadyRef.current) {
      flushRenderPayload({ immediate: !isStreaming })
    }
  }, [description, flushRenderPayload, html, isStreaming, title])

  return (
    <iframe
      ref={iframeRef}
      title={title}
      srcDoc={srcDoc}
      sandbox="allow-scripts allow-popups"
      className="w-full overflow-hidden rounded-[calc(var(--radius)*1.25)] border border-border/50 bg-transparent"
      style={{ height: frameHeight, minHeight: 1, overflow: "hidden" }}
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

function createWidgetReadmeSummary(data: {
  arguments: Record<string, unknown>
}) {
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
      const modules = getArrayValue(
        normalizeToolArguments(data.arguments),
        "modules"
      ).flatMap((module) => (typeof module === "string" ? [module] : []))
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
      return createWidgetSummary({ arguments: args, details: data.details })
        .title
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
        getStringValue(details, "title") ??
        getStringValue(args, "title") ??
        "Widget"
      const description =
        getStringValue(details, "description") ??
        getStringValue(args, "description") ??
        null
      const liveHtml = getStreamingWidgetHtml({
        previewHtml: data.previewHtml,
        outputContent: data.outputContent,
        outputSegments: data.outputSegments,
        rawArguments: data.rawArguments,
      })
      const html = data.isRunning
        ? liveHtml || getStringValue(args, "html") || ""
        : getStringValue(details, "html") ||
          getStringValue(args, "html") ||
          liveHtml ||
          ""

      if (!html.trim()) {
        return (
          <ToolDetailSection title="Content">
            <ExpandableOutput
              value={data.outputContent}
              failed={!data.succeeded}
            />
          </ToolDetailSection>
        )
      }

      return (
        <div className="space-y-3">
          <ToolDetailSurface className="tool-timeline-detail-surface-flat tool-timeline-detail-surface-borderless">
            <WidgetSandbox
              title={title}
              description={description}
              html={html}
              isStreaming={data.isRunning}
            />
          </ToolDetailSurface>
          <ToolInfoSection title="Content" defaultOpen={false}>
            <ExpandableOutput value={html} failed={false} />
          </ToolInfoSection>
        </div>
      )
    },
  }
}
