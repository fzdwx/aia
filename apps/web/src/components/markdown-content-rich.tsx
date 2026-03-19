import { memo } from "react"
import NodeRenderer from "markstream-react"

import { useTheme } from "@/components/theme-provider"
import { cn } from "@/lib/utils"

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

    return (
      <div className={cn("markdown-content", className)}>
        <NodeRenderer
          content={content}
          final={!streaming}
          isDark={resolvedTheme === "dark"}
          mermaidProps={{
            showCollapseButton: false,
            showCopyButton: true,
            showExportButton: false,
            showFullscreenButton: false,
            showHeader: false,
            showModeToggle: false,
            showZoomControls: false,
          }}
        />
      </div>
    )
  }
)
