import { memo } from "react"

import { MarkdownRenderer } from "@/components/markdown-content-rich"

type MarkdownContentProps = {
  content: string
  className?: string
  streaming?: boolean
}

export const MarkdownContent = memo((props: MarkdownContentProps) => {
  return <MarkdownRenderer {...props} />
})
