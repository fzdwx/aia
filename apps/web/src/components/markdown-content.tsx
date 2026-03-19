import { Suspense, lazy, memo } from "react"

import { cn } from "@/lib/utils"

const LazyMarkdownRenderer = lazy(async () => {
  const module = await import("@/components/markdown-content-rich")
  return { default: module.MarkdownRenderer }
})

type MarkdownContentProps = {
  content: string
  className?: string
  streaming?: boolean
}

function MarkdownFallback({
  content,
  className,
}: Pick<MarkdownContentProps, "content" | "className">) {
  return (
    <div className={cn("markdown-content", className)}>
      <div className="break-words whitespace-pre-wrap text-inherit">
        {content}
      </div>
    </div>
  )
}

export const MarkdownContent = memo((props: MarkdownContentProps) => {
  return (
    <Suspense
      fallback={
        <MarkdownFallback content={props.content} className={props.className} />
      }
    >
      <LazyMarkdownRenderer {...props} />
    </Suspense>
  )
})
