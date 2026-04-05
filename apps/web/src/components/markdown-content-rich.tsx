import {
  createElement,
  memo,
  useMemo,
  useRef,
  type ComponentPropsWithoutRef,
} from "react"
import { Check, Copy } from "lucide-react"
import { cjk } from "@streamdown/cjk"
import { createCodePlugin } from "@streamdown/code"
import { math } from "@streamdown/math"
import { mermaid } from "@streamdown/mermaid"
import { Streamdown } from "streamdown"
import type { IconMap } from "streamdown"

import {
  computeStreamingMarkdownBlocks,
  type StreamingMarkdownBlockCache,
} from "@/components/markdown-streaming-blocks"
import { cn } from "@/lib/utils"

const OVERSIZE_STREAMING_MARKDOWN_THRESHOLD = 16_000

const code = createCodePlugin({
  themes: ["github-light", "github-dark"],
})

const markdownIcons = {
  CheckIcon: Check,
  CopyIcon: Copy,
} satisfies Partial<IconMap>

type MarkdownTag =
  | "a"
  | "blockquote"
  | "code"
  | "em"
  | "h1"
  | "h2"
  | "h3"
  | "h4"
  | "h5"
  | "h6"
  | "hr"
  | "p"
  | "strong"

type MarkdownComponentProps<T extends MarkdownTag> =
  ComponentPropsWithoutRef<T> & {
    node?: unknown
  }

function withClasses<T extends MarkdownTag>(tag: T, classes: string) {
  return ({ className, ...props }: MarkdownComponentProps<T>) =>
    createElement(tag, {
      ...props,
      className: cn(classes, className),
    })
}

const markdownComponents = {
  p: withClasses("p", "mb-2.5"),
  strong: withClasses("strong", "font-semibold"),
  em: withClasses("em", "italic"),
  a: withClasses("a", "underline underline-offset-4"),
  h1: withClasses("h1", "mb-2 text-[1em] font-semibold"),
  h2: withClasses("h2", "mb-2 text-[1em] font-semibold"),
  h3: withClasses("h3", "mb-2 text-[1em] font-semibold"),
  h4: withClasses("h4", "mb-2 text-[1em] font-semibold"),
  h5: withClasses("h5", "mb-2 text-[1em] font-semibold"),
  h6: withClasses("h6", "mb-2 text-[1em] font-semibold"),
  blockquote: withClasses(
    "blockquote",
    "mb-3 border-l-2 border-border/40 pl-4 text-muted-foreground"
  ),
  hr: withClasses("hr", "my-4 border-border/40"),
  inlineCode: withClasses("code", "inline-code"),
} as const

function StreamingMarkdownRenderer({
  content,
  className,
}: {
  content: string
  className?: string
}) {
  const cacheRef = useRef<StreamingMarkdownBlockCache | null>(null)

  const blockCache = useMemo(() => {
    const next = computeStreamingMarkdownBlocks(content, cacheRef.current)
    cacheRef.current = next
    return next
  }, [content])

  const parseMarkdownIntoBlocksFn = useMemo(
    () => () => blockCache.blocks,
    [blockCache.blocks]
  )

  return (
    <div className={cn("markdown-content", className)}>
      <Streamdown
        mode="streaming"
        components={markdownComponents}
        icons={markdownIcons}
        plugins={{ cjk, code, math, mermaid }}
        parseMarkdownIntoBlocksFn={parseMarkdownIntoBlocksFn}
      >
        {content}
      </Streamdown>
    </div>
  )
}

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
    const shouldUsePlainTextFallback =
      streaming && content.length >= OVERSIZE_STREAMING_MARKDOWN_THRESHOLD

    if (streaming && !shouldUsePlainTextFallback) {
      return (
        <StreamingMarkdownRenderer content={content} className={className} />
      )
    }

    return (
      <div className={cn("markdown-content", className)}>
        {shouldUsePlainTextFallback ? (
          <div className="break-words whitespace-pre-wrap">{content}</div>
        ) : (
          <Streamdown
            mode="static"
            components={markdownComponents}
            icons={markdownIcons}
            plugins={{ cjk, code, math, mermaid }}
            remend={streaming ? undefined : { bold: false, boldItalic: false }}
          >
            {content}
          </Streamdown>
        )}
      </div>
    )
  }
)
