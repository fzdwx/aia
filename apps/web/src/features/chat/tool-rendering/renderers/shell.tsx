import { useEffect, useRef } from "react"

import type { ToolOutputSegment } from "@/lib/types"
import { getToolDisplayPath, normalizeToolArguments } from "@/lib/tool-display"

import { toolTimelineCopy } from "../../tool-timeline-copy"

import type { ToolRenderer } from "../types"
import { getStringValue, truncateInline } from "../helpers"

function buildShellOutput(data: {
  details?: Record<string, unknown>
  outputContent: string
  outputSegments?: ToolOutputSegment[]
}): string | null {
  if (data.outputContent.trim().length > 0) {
    return data.outputContent
  }

  if ((data.outputSegments?.length ?? 0) > 0) {
    return data.outputSegments?.map((segment) => segment.text).join("") ?? null
  }

  const stdout = getStringValue(data.details, "stdout")
  const stderr = getStringValue(data.details, "stderr")
  const parts = [stdout, stderr].filter(
    (value): value is string => typeof value === "string" && value.length > 0
  )

  return parts.length > 0 ? parts.join("\n") : null
}

function firstShellLine(data: {
  details?: Record<string, unknown>
  outputContent: string
  outputSegments?: ToolOutputSegment[]
}): string | null {
  const output = buildShellOutput(data)
  if (!output) return null

  const firstLine = output
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.length > 0)

  return firstLine ? truncateInline(firstLine, 96) : null
}

function ShellOutputBody({
  command,
  output,
  segments,
  isRunning,
}: {
  command: string
  output: string | null
  segments: ToolOutputSegment[]
  isRunning: boolean
}) {
  const preRef = useRef<HTMLPreElement | null>(null)
  const shouldFollowRef = useRef(true)
  const followTrigger =
    segments.length > 0
      ? segments
          .map((segment) => `${segment.stream}:${segment.text}`)
          .join("\u0000")
      : (output ?? "")

  useEffect(() => {
    const element = preRef.current
    if (!element) return

    if (isRunning) {
      shouldFollowRef.current = true
      element.scrollTop = element.scrollHeight
    }

    const handleScroll = () => {
      const distance =
        element.scrollHeight - element.scrollTop - element.clientHeight
      shouldFollowRef.current = distance <= 12
    }

    handleScroll()
    element.addEventListener("scroll", handleScroll)
    return () => {
      element.removeEventListener("scroll", handleScroll)
    }
  }, [isRunning])

  useEffect(() => {
    void followTrigger
    const element = preRef.current
    if (!element || !shouldFollowRef.current) return
    element.scrollTop = element.scrollHeight
  }, [followTrigger])

  const hasStreamingSegments = segments.length > 0
  let segmentOffset = 0

  return (
    <pre ref={preRef} className="tool-timeline-shell-pre">
      <span className="tool-timeline-shell-command">$ {command}</span>
      {hasStreamingSegments ? (
        <>
          {"\n\n"}
          {segments.map((segment) => {
            const key = `${segment.stream}-${segmentOffset}`
            segmentOffset += segment.text.length

            return (
              <span
                key={key}
                className={
                  segment.stream === "stderr"
                    ? "tool-timeline-shell-segment tool-timeline-shell-segment-stderr"
                    : "tool-timeline-shell-segment tool-timeline-shell-segment-stdout"
                }
                data-stream={segment.stream}
              >
                {segment.text}
              </span>
            )
          })}
        </>
      ) : output ? (
        `\n\n${output}`
      ) : null}
    </pre>
  )
}

export function createShellRenderer(): ToolRenderer {
  return {
    matches: (toolName) => toolName === "Shell",
    renderTitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const descriptionValue = getStringValue(args, "description")
      const description = descriptionValue
        ? truncateInline(descriptionValue, 96)
        : null
      const command = truncateInline(
        getStringValue(args, "command", "cmd") ??
          getToolDisplayPath(data.toolName, data.details, args),
        96
      )

      return description || command || toolTimelineCopy.toolName.shell
    },
    renderSubtitle(data) {
      const args = normalizeToolArguments(data.arguments)
      const description = getStringValue(args, "description")
      const command =
        getStringValue(data.details, "command") ??
        getStringValue(args, "command", "cmd")

      return (
        truncateInline(
          description ?? command ?? firstShellLine(data) ?? "",
          96
        ) || null
      )
    },
    renderDetails(data) {
      const args = normalizeToolArguments(data.arguments)
      const command =
        getStringValue(data.details, "command") ??
        getStringValue(args, "command", "cmd")

      if (!command) return null

      const output = buildShellOutput(data)
      const segments = data.outputSegments ?? []

      return (
        <section
          className="tool-timeline-detail-section tool-timeline-shell-detail"
          data-tool-detail-kind="content"
          data-tool-detail-tone="output"
        >
          <div className="tool-timeline-detail-body tool-timeline-shell-body">
            <ShellOutputBody
              command={command}
              output={output}
              segments={segments}
              isRunning={data.isRunning ?? false}
            />
          </div>
        </section>
      )
    },
  }
}
