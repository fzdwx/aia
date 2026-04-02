import { useEffect, useState } from "react"

import { MarkdownContent } from "@/components/markdown-content"
import type { TurnBlock } from "@/lib/types"

import { MessageCopyButton } from "./message-copy-button"

export const MESSAGE_READING_MEASURE = "w-full"

function findLastNonEmptyLine(content: string): string {
  for (let index = content.length - 1; index >= 0; index -= 1) {
    if (content[index] !== "\n" && content[index] !== "\r") {
      let start = index
      while (
        start > 0 &&
        content[start - 1] !== "\n" &&
        content[start - 1] !== "\r"
      ) {
        start -= 1
      }
      return content.slice(start, index + 1).trim()
    }
  }

  return ""
}

export function AssistantTextBlock({
  content,
  streaming = false,
}: {
  content: string
  streaming?: boolean
}) {
  return (
    <div data-component="text-part" className="group/text-part">
      <div
        data-slot="text-part-body"
        className={`${MESSAGE_READING_MEASURE} group/text-part-body relative`}
      >
        <div
          data-slot="text-part-copy-wrapper"
          className="pointer-events-none absolute top-0 right-0 z-10 opacity-0 transition-opacity duration-150 group-focus-within/text-part-body:pointer-events-auto group-focus-within/text-part-body:opacity-100 group-hover/text-part-body:pointer-events-auto group-hover/text-part-body:opacity-100"
        >
          <MessageCopyButton
            content={content}
            copyLabel="Copy response"
            copiedLabel="Copied"
          />
        </div>
        <MarkdownContent
          content={content}
          streaming={streaming}
          className="text-body-sm leading-body-sm pr-10 text-pretty text-foreground/92"
        />
      </div>
    </div>
  )
}

export function ThinkingBlock({
  content,
  isStreaming = false,
}: {
  content: string
  isStreaming?: boolean
}) {
  const [open, setOpen] = useState(isStreaming)
  const lastLine = findLastNonEmptyLine(content)

  useEffect(() => {
    if (isStreaming) {
      setOpen(true)
    }
  }, [isStreaming])

  return (
    <div
      data-component="reasoning-part"
      className={`${MESSAGE_READING_MEASURE} py-1`}
    >
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="text-body-sm leading-body-sm flex w-full items-baseline gap-2 text-left"
      >
        {isStreaming ? (
          <span data-slot="tool-title">Thinking</span>
        ) : (
          <>
            <span data-slot="tool-title">Thought</span>
            {!open && lastLine ? (
              <span
                data-slot="tool-subtitle"
                className="max-w-[400px] truncate"
              >
                {lastLine}
              </span>
            ) : null}
          </>
        )}
      </button>
      {open ? (
        <div className="text-body-sm leading-body-sm mt-2.5 border-l-2 border-border/30 pl-3">
          <MarkdownContent
            content={content}
            streaming={isStreaming}
            className="opacity-50"
          />
        </div>
      ) : null}
    </div>
  )
}

export function BlockRenderer({ block }: { block: TurnBlock }) {
  switch (block.kind) {
    case "thinking":
      return <ThinkingBlock content={block.content} />
    case "assistant":
      return <AssistantTextBlock content={block.content} />
    case "failure":
      return (
        <div
          className={`${MESSAGE_READING_MEASURE} text-caption rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 font-medium text-destructive`}
        >
          {block.message}
        </div>
      )
    case "cancelled":
      return (
        <div
          className={`${MESSAGE_READING_MEASURE} text-caption rounded-lg border border-border/40 bg-muted/40 px-3 py-2 font-medium text-muted-foreground`}
        >
          {block.message}
        </div>
      )
    case "tool_invocation":
      return null
  }
}

export function UserMessageBlock({ content }: { content: string }) {
  return (
    <div
      data-component="user-message"
      className="group/user-message flex w-full justify-start"
    >
      <div className="flex max-w-full flex-col items-start">
        <div
          data-slot="user-message-body"
          className="group/user-message-body relative w-full max-w-full rounded-md border border-border/45 bg-background/88 px-3 py-2.5"
        >
          <div
            data-slot="user-message-copy-wrapper"
            className="pointer-events-none absolute top-2 right-2 z-10 opacity-0 transition-opacity duration-150 group-focus-within/user-message-body:pointer-events-auto group-focus-within/user-message-body:opacity-100 group-hover/user-message-body:pointer-events-auto group-hover/user-message-body:opacity-100"
          >
            <MessageCopyButton
              content={content}
              copyLabel="Copy message"
              copiedLabel="Copied"
            />
          </div>
          <div
            data-slot="user-message-text"
            className="text-body-sm leading-body-sm max-w-full pr-10 text-pretty text-foreground/92"
          >
            <MarkdownContent content={content} />
          </div>
        </div>
      </div>
    </div>
  )
}
