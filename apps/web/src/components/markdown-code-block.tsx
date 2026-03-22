import { type ReactNode, useEffect, useMemo, useRef, useState } from "react"
import {
  D2BlockNode,
  MarkdownCodeBlockNode,
  type NodeComponentProps,
  type CodeBlockNodeProps,
  InfographicBlockNode,
  MermaidBlockNode,
  PreCodeNode,
  languageMap,
  normalizeLanguageIdentifier,
} from "markstream-react"

const COPY_RESET_DELAY_MS = 1200

const DISPLAY_LANGUAGE_LABELS: Record<string, string> = {
  bash: "Bash",
  c: "C",
  cpp: "C++",
  csharp: "C#",
  css: "CSS",
  d2: "D2",
  d2lang: "D2",
  go: "Go",
  graphql: "GraphQL",
  html: "HTML",
  java: "Java",
  javascript: "JavaScript",
  json: "JSON",
  jsx: "JSX",
  markdown: "Markdown",
  md: "Markdown",
  mermaid: "Mermaid",
  objectivec: "Objective-C",
  plaintext: "Text",
  python: "Python",
  ruby: "Ruby",
  rust: "Rust",
  shell: "Shell",
  sql: "SQL",
  svg: "SVG",
  swift: "Swift",
  toml: "TOML",
  ts: "TypeScript",
  tsx: "TSX",
  typescript: "TypeScript",
  xml: "XML",
  yaml: "YAML",
  yml: "YAML",
}

function formatLanguageLabel(language?: string | null): string {
  const normalized = normalizeLanguageIdentifier(language)
  const mapped = DISPLAY_LANGUAGE_LABELS[normalized] ?? languageMap[normalized]

  if (mapped) {
    return mapped
      .split(/[\s_-]+/)
      .filter(Boolean)
      .map((segment) => {
        if (segment.toUpperCase() === segment) {
          return segment
        }

        return segment.charAt(0).toUpperCase() + segment.slice(1)
      })
      .join(" ")
  }

  if (!normalized || normalized === "plaintext") {
    return "Text"
  }

  return normalized
    .split(/[\s_-]+/)
    .filter(Boolean)
    .map((segment) => segment.charAt(0).toUpperCase() + segment.slice(1))
    .join(" ")
}

async function copyText(value: string): Promise<boolean> {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(value)
      return true
    } catch {
      // Fall back to document-based copy below.
    }
  }

  if (typeof document === "undefined") {
    return false
  }

  const textarea = document.createElement("textarea")
  textarea.value = value
  textarea.setAttribute("readonly", "")
  textarea.style.position = "fixed"
  textarea.style.opacity = "0"
  textarea.style.pointerEvents = "none"
  document.body.appendChild(textarea)
  textarea.select()

  try {
    return document.execCommand("copy")
  } catch {
    return false
  } finally {
    document.body.removeChild(textarea)
  }
}

export function MarkdownCodeBlock({
  node,
  ctx,
}: NodeComponentProps<CodeBlockNodeProps["node"]>) {
  const normalizedLanguage = useMemo(
    () => normalizeLanguageIdentifier(node.language),
    [node.language]
  )
  const languageLabel = useMemo(
    () => formatLanguageLabel(node.language),
    [node.language]
  )
  const diagramKind =
    normalizedLanguage === "d2lang" ? "d2" : normalizedLanguage
  const resetTimerRef = useRef<number | null>(null)
  const [copied, setCopied] = useState(false)

  const renderDiagramBlock = (content: ReactNode) => {
    return (
      <div className="chat-diagram-block" data-diagram-kind={diagramKind}>
        <div className="chat-diagram-block-label">{languageLabel}</div>
        <div className="chat-diagram-block-body">{content}</div>
      </div>
    )
  }

  useEffect(() => {
    return () => {
      if (resetTimerRef.current !== null) {
        window.clearTimeout(resetTimerRef.current)
      }
    }
  }, [])

  if (ctx?.renderCodeBlocksAsPre) {
    return <PreCodeNode node={node} />
  }

  if (normalizedLanguage === "mermaid") {
    const MermaidComponent = ctx?.customComponents?.mermaid

    return renderDiagramBlock(
      MermaidComponent ? (
        <MermaidComponent
          isDark={ctx.isDark}
          node={node}
          {...(ctx.mermaidProps ?? {})}
        />
      ) : (
        <MermaidBlockNode
          isDark={ctx?.isDark}
          loading={Boolean(node.loading)}
          node={node}
          {...(ctx?.mermaidProps ?? {})}
        />
      )
    )
  }

  if (normalizedLanguage === "infographic") {
    const InfographicComponent = ctx?.customComponents?.infographic

    return renderDiagramBlock(
      InfographicComponent ? (
        <InfographicComponent
          isDark={ctx?.isDark}
          node={node}
          {...(ctx?.infographicProps ?? {})}
        />
      ) : (
        <InfographicBlockNode
          isDark={ctx?.isDark}
          loading={Boolean(node.loading)}
          node={node}
          {...(ctx?.infographicProps ?? {})}
        />
      )
    )
  }

  if (normalizedLanguage === "d2" || normalizedLanguage === "d2lang") {
    const D2Component = ctx?.customComponents?.d2

    return renderDiagramBlock(
      D2Component ? (
        <D2Component
          isDark={ctx?.isDark}
          node={node}
          {...(ctx?.d2Props ?? {})}
        />
      ) : (
        <D2BlockNode
          isDark={ctx?.isDark}
          loading={Boolean(node.loading)}
          node={node}
          {...(ctx?.d2Props ?? {})}
        />
      )
    )
  }

  const handleCopy = async () => {
    const success = await copyText(node.code)

    if (!success) {
      return
    }

    if (resetTimerRef.current !== null) {
      window.clearTimeout(resetTimerRef.current)
    }

    setCopied(true)
    ctx?.events.onCopy?.(node.code)

    if (typeof window !== "undefined") {
      resetTimerRef.current = window.setTimeout(() => {
        setCopied(false)
        resetTimerRef.current = null
      }, COPY_RESET_DELAY_MS)
    }
  }

  return (
    <div
      className="chat-code-block"
      data-language={normalizedLanguage}
      data-state={copied ? "copied" : "idle"}
    >
      <div className="chat-code-block-header">
        <span className="chat-code-block-language">{languageLabel}</span>
        <div className="chat-code-block-actions">
          <button
            aria-label={copied ? "Copied code" : "Copy code"}
            className="chat-code-block-copy"
            onClick={handleCopy}
            type="button"
          >
            {copied ? "Copied" : "Copy"}
          </button>
        </div>
      </div>
      <MarkdownCodeBlockNode
        darkTheme="min-dark"
        isDark={ctx?.isDark}
        lightTheme="min-light"
        loading={Boolean(node.loading)}
        maxWidth={ctx?.codeBlockThemes?.maxWidth}
        minWidth={ctx?.codeBlockThemes?.minWidth}
        node={node}
        onCopy={ctx?.events.onCopy}
        stream={ctx?.codeBlockStream}
        showCopyButton={false}
        showExpandButton={false}
        showFontSizeButtons={false}
        showHeader={false}
        showPreviewButton={false}
        showTooltips={false}
      />
    </div>
  )
}
