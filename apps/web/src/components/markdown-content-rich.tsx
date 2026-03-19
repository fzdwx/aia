import { memo, useMemo, type ReactNode } from "react"
import NodeRenderer, {
  type NodeComponentProps,
  setCustomComponents,
} from "markstream-react"
import {
  getMarkdown,
  parseMarkdownToStructure,
  type ParsedNode,
} from "stream-markdown-parser"

import { MarkdownCodeBlock } from "@/components/markdown-code-block"
import { useTheme } from "@/components/theme-provider"
import { cn } from "@/lib/utils"

type MarkstreamLinkNode = {
  type: "link"
  href: string
  title: string | null
  children?: ParsedNode[]
}

const CHAT_MARKDOWN_ID = "chat-markdown"
const chatMarkdown = getMarkdown(CHAT_MARKDOWN_ID)

const CHAT_CODE_BLOCK_PROPS = {
  enableFontSizeControl: false,
  showCollapseButton: false,
  showCopyButton: true,
  showExpandButton: false,
  showFontSizeButtons: false,
  showHeader: true,
  showPreviewButton: false,
} as const

const CHAT_MERMAID_PROPS = {
  showCollapseButton: false,
  showCopyButton: true,
  showExportButton: false,
  showFullscreenButton: false,
  showHeader: false,
  showModeToggle: false,
  showZoomControls: false,
} as const

function renderNodeChildren(
  children: ParsedNode[] | undefined,
  ctx: NodeComponentProps["ctx"],
  renderNode: NodeComponentProps["renderNode"],
  indexKey: NodeComponentProps["indexKey"],
  scope: string
): ReactNode {
  if (!children || !ctx || !renderNode) return null

  return children.map((child, index) =>
    renderNode(child, `${String(indexKey ?? scope)}-${scope}-${index}`, ctx)
  )
}

function isExternalHref(href: string): boolean {
  return (
    href.startsWith("http://") ||
    href.startsWith("https://") ||
    href.startsWith("mailto:") ||
    href.startsWith("tel:")
  )
}

function ChatLink({
  node,
  ctx,
  renderNode,
  indexKey,
}: NodeComponentProps<MarkstreamLinkNode>) {
  const external = isExternalHref(node.href)

  return (
    <a
      className="link-node chat-markdown-link"
      href={node.href}
      rel={external ? "noreferrer" : undefined}
      target={external ? "_blank" : undefined}
      title={node.title ?? undefined}
    >
      {renderNodeChildren(node.children, ctx, renderNode, indexKey, "link")}
    </a>
  )
}

setCustomComponents(CHAT_MARKDOWN_ID, {
  code_block: MarkdownCodeBlock,
  link: ChatLink,
})

export const MarkdownRenderer = memo(
  ({
    content,
    className,
    streaming = false,
  }: {
    content: string
    className?: string
    streaming?: boolean
  }) => {
    const { resolvedTheme } = useTheme()
    const parsedNodes = useMemo<ParsedNode[]>(() => {
      if (!streaming) {
        return []
      }

      return parseMarkdownToStructure(content, chatMarkdown, {
        final: false,
      })
    }, [content, streaming])

    return (
      <div className={cn("markdown-content", className)}>
        <NodeRenderer
          batchRendering
          codeBlockProps={CHAT_CODE_BLOCK_PROPS}
          content={streaming ? undefined : content}
          customId={CHAT_MARKDOWN_ID}
          deferNodesUntilVisible
          final={!streaming}
          isDark={resolvedTheme === "dark"}
          mermaidProps={CHAT_MERMAID_PROPS}
          nodes={streaming ? parsedNodes : undefined}
          showTooltips={false}
          typewriter={false}
          viewportPriority
        />
      </div>
    )
  }
)
