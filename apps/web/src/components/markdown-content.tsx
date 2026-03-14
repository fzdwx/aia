import { memo } from "react"
import { Streamdown } from "streamdown"
import { cjk } from "@streamdown/cjk"

import { cn } from "@/lib/utils"

const markdownClassName = cn(
  "text-[14px] leading-[1.75] text-foreground/85",
  "[&>*:last-child]:mb-0",
  "[&_blockquote]:mb-3 [&_dl]:mb-3 [&_ol]:mb-2 [&_p]:mb-2.5 [&_pre]:mb-3 [&_table]:mb-3 [&_ul]:mb-2",
  "[&_strong]:font-semibold",
  "[&_em]:italic",
  "[&_a]:underline [&_a]:underline-offset-4",
  "[&_ol]:list-outside [&_ol]:list-decimal [&_ol]:pl-6",
  "[&_ul]:list-outside [&_ul]:list-disc [&_ul]:pl-6",
  "[&_li_ol]:mt-1 [&_li_ol]:mb-0 [&_li_ul]:mt-1 [&_li_ul]:mb-0 [&_ol>li]:mb-1.5 [&_ul>li]:mb-1",
  "[&_h1]:mb-2 [&_h1]:text-[1em] [&_h1]:font-semibold",
  "[&_h2]:mb-2 [&_h2]:text-[1em] [&_h2]:font-semibold",
  "[&_h3]:mb-2 [&_h3]:text-[1em] [&_h3]:font-semibold",
  "[&_h4]:mb-2 [&_h4]:text-[1em] [&_h4]:font-semibold",
  "[&_h5]:mb-2 [&_h5]:text-[1em] [&_h5]:font-semibold",
  "[&_h6]:mb-2 [&_h6]:text-[1em] [&_h6]:font-semibold",
  "[&_blockquote]:border-l-2 [&_blockquote]:border-border/40 [&_blockquote]:pl-4 [&_blockquote]:text-muted-foreground",
  "[&_hr]:my-4 [&_hr]:border-border/40",
  // Table — layout basics (Streamdown overrides handled via data-streamdown selectors in index.css)
  "[&_table]:w-full [&_table]:border-collapse",
  // Code block — typography & overflow (border/radius handled via data-streamdown selector in index.css)
  "[&_pre]:overflow-x-auto [&_pre]:px-3 [&_pre]:py-2",
  "[&_pre]:font-mono [&_pre]:text-[13px] [&_pre]:leading-[1.6] [&_pre]:break-words [&_pre]:whitespace-pre-wrap",
  "[&_pre_span]:whitespace-break-spaces",
  // Inline code
  "[&_:not(pre)>code]:rounded-[4px] [&_:not(pre)>code]:bg-muted [&_:not(pre)>code]:px-1.5 [&_:not(pre)>code]:py-0.5 [&_:not(pre)>code]:font-mono [&_:not(pre)>code]:text-[0.85em]"
)

export const MarkdownContent = memo(
  ({
    content,
    className,
    streaming = false,
  }: {
    content: string
    className?: string
    streaming?: boolean
  }) => (
    <div className={cn(markdownClassName, className)}>
      <Streamdown
        plugins={{ cjk }}
        remend={streaming ? undefined : { bold: false, boldItalic: false }}
      >
        {content}
      </Streamdown>
    </div>
  )
)
