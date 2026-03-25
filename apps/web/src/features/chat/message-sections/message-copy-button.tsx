import { Check, Copy } from "lucide-react"
import { useEffect, useRef, useState } from "react"

import { copyTextToClipboard } from "@/lib/clipboard"
import { cn } from "@/lib/utils"

const COPY_RESET_DELAY_MS = 1500

export function MessageCopyButton({
  content,
  copyLabel,
  copiedLabel,
  className,
}: {
  content: string
  copyLabel: string
  copiedLabel: string
  className?: string
}) {
  const [copied, setCopied] = useState(false)
  const resetTimerRef = useRef<number | null>(null)

  useEffect(() => {
    return () => {
      if (resetTimerRef.current !== null) {
        window.clearTimeout(resetTimerRef.current)
      }
    }
  }, [])

  const handleCopy = async () => {
    if (!content.trim()) return

    const success = await copyTextToClipboard(content)
    if (!success) return

    setCopied(true)

    if (resetTimerRef.current !== null) {
      window.clearTimeout(resetTimerRef.current)
    }

    resetTimerRef.current = window.setTimeout(() => {
      setCopied(false)
      resetTimerRef.current = null
    }, COPY_RESET_DELAY_MS)
  }

  return (
    <button
      type="button"
      onClick={() => {
        void handleCopy()
      }}
      data-slot="message-copy-button"
      aria-label={copied ? copiedLabel : copyLabel}
      title={copied ? copiedLabel : copyLabel}
      className={cn(
        "inline-flex items-center justify-center rounded-md border border-border/35 bg-background/88 p-1 text-muted-foreground shadow-none transition-colors hover:text-foreground focus-visible:text-foreground focus-visible:outline-none",
        className
      )}
    >
      {copied ? <Check className="size-3.5" /> : <Copy className="size-3.5" />}
    </button>
  )
}
