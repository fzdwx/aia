import { createElement, memo, type ComponentPropsWithoutRef } from "react"
import { Check, Copy } from "lucide-react"
import { cjk } from "@streamdown/cjk"
import { createCodePlugin } from "@streamdown/code"
import { math } from "@streamdown/math"
import { mermaid } from "@streamdown/mermaid"
import { Streamdown } from "streamdown"
import type { IconMap } from "streamdown"

import { cn } from "@/lib/utils"

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
  inlineCode: withClasses(
    "code",
    "rounded-[4px] border border-border/40 bg-muted/70 px-1.5 py-0.5 font-mono text-[0.85em] text-foreground"
  ),
} as const

export const MarkdownRenderer = memo(
  ({
    content,
    className,
    streaming = false,
  }: {
    content: string
    className?: string
    streaming?: boolean
  }) => (
    <div className={cn("markdown-content", className)}>
      <Streamdown
        components={markdownComponents}
        icons={markdownIcons}
        plugins={{ cjk, code, math, mermaid }}
        remend={streaming ? undefined : { bold: false, boldItalic: false }}
      >
        {content}
      </Streamdown>
    </div>
  )
)
