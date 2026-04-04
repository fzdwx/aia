import { useEffect, useRef } from "react"

import type { ToolOutputSegment } from "@/lib/types"

import { stripAnsiSequences } from "./shell-output"

type ShellOutputBodyProps = {
  command: string
  output: string | null
  segments: ToolOutputSegment[]
  isRunning: boolean
}

export function ShellOutputBody({
  command,
  output,
  segments,
  isRunning,
}: ShellOutputBodyProps) {
  const preRef = useRef<HTMLPreElement | null>(null)
  const shouldFollowRef = useRef(true)
  // segments 可能在工具完成后被清理，此时使用 output
  const hasStreamingSegments = segments.length > 0
  const followTrigger = hasStreamingSegments
    ? segments
        .map((segment) => `${segment.stream}:${stripAnsiSequences(segment.text)}`)
        .join("\u0000")
    : stripAnsiSequences(output ?? "")

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
                {stripAnsiSequences(segment.text)}
              </span>
            )
          })}
        </>
      ) : output ? (
        `\n\n${stripAnsiSequences(output)}`
      ) : null}
    </pre>
  )
}
