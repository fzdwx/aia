import { useCallback, useEffect, useRef, useState } from "react"
import {
  connectEvents,
  fetchHistory,
  fetchProviders,
  submitTurn as apiSubmitTurn,
} from "@/lib/api"
import type {
  ChatState,
  ProviderInfo,
  SseEvent,
  StreamingTurn,
  TurnLifecycle,
  TurnStatus,
} from "@/lib/types"

export function useChat() {
  const [turns, setTurns] = useState<TurnLifecycle[]>([])
  const [streamingTurn, setStreamingTurn] = useState<StreamingTurn | null>(null)
  const [chatState, setChatState] = useState<ChatState>("idle")
  const [provider, setProvider] = useState<ProviderInfo | null>(null)
  const [error, setError] = useState<string | null>(null)
  const pendingPromptRef = useRef<string | null>(null)

  // Connect to global SSE on mount
  useEffect(() => {
    fetchProviders()
      .then(setProvider)
      .catch(() => {})

    fetchHistory()
      .then(setTurns)
      .catch(() => {})

    const cleanup = connectEvents((event: SseEvent) => {
      switch (event.type) {
        case "status": {
          const status = event.data.status as TurnStatus
          if (status === "waiting") {
            // Start of a new turn — grab the pending prompt
            const prompt = pendingPromptRef.current ?? ""
            pendingPromptRef.current = null
            setChatState("active")
            setStreamingTurn({
              userMessage: prompt,
              thinkingText: "",
              assistantText: "",
              status: "waiting",
              toolOutputs: [],
            })
          } else {
            setStreamingTurn((prev) =>
              prev ? { ...prev, status } : null,
            )
          }
          break
        }
        case "stream": {
          const data = event.data
          if (data.kind === "thinking_delta") {
            setStreamingTurn((prev) =>
              prev
                ? { ...prev, thinkingText: prev.thinkingText + data.text }
                : null,
            )
          } else if (data.kind === "text_delta") {
            setStreamingTurn((prev) =>
              prev
                ? { ...prev, assistantText: prev.assistantText + data.text }
                : null,
            )
          } else if (data.kind === "tool_output_delta") {
            setStreamingTurn((prev) => {
              if (!prev) return null
              const outputs = [...prev.toolOutputs]
              const idx = outputs.findIndex(
                (t) => t.invocationId === data.invocation_id,
              )
              if (idx >= 0) {
                outputs[idx] = {
                  ...outputs[idx],
                  output: outputs[idx].output + data.text,
                }
              } else {
                outputs.push({
                  invocationId: data.invocation_id,
                  output: data.text,
                })
              }
              return { ...prev, toolOutputs: outputs }
            })
          }
          break
        }
        case "turn_completed": {
          setTurns((prev) => [...prev, event.data])
          setStreamingTurn(null)
          setChatState("idle")
          setError(null)
          break
        }
        case "error": {
          setError(event.data.message)
          setStreamingTurn(null)
          setChatState("idle")
          break
        }
      }
    })

    return cleanup
  }, [])

  const handleSubmitTurn = useCallback(
    (prompt: string) => {
      if (chatState === "active") return
      setError(null)
      pendingPromptRef.current = prompt
      apiSubmitTurn(prompt).catch((err: unknown) => {
        setError(err instanceof Error ? err.message : "Network error")
        pendingPromptRef.current = null
      })
    },
    [chatState],
  )

  return {
    turns,
    streamingTurn,
    chatState,
    provider,
    error,
    submitTurn: handleSubmitTurn,
  }
}
