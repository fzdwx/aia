import { useCallback, useEffect, useRef, useState } from "react"
import { fetchHistory, fetchProviders, submitTurn } from "@/lib/api"
import type {
  ChatState,
  ProviderInfo,
  SseEvent,
  StreamingTurn,
  TurnLifecycle,
} from "@/lib/types"

export function useChat() {
  const [turns, setTurns] = useState<TurnLifecycle[]>([])
  const [streamingTurn, setStreamingTurn] = useState<StreamingTurn | null>(null)
  const [chatState, setChatState] = useState<ChatState>("idle")
  const [provider, setProvider] = useState<ProviderInfo | null>(null)
  const [error, setError] = useState<string | null>(null)
  const controllerRef = useRef<AbortController | null>(null)

  // Load provider info and history on mount
  useEffect(() => {
    fetchProviders()
      .then(setProvider)
      .catch(() => {
        /* server not running yet */
      })

    fetchHistory()
      .then(setTurns)
      .catch(() => {
        /* server not running yet */
      })
  }, [])

  const handleSubmitTurn = useCallback(
    (prompt: string) => {
      if (chatState === "streaming") return

      setError(null)
      setChatState("streaming")
      setStreamingTurn({ thinkingText: "", assistantText: "" })

      const controller = submitTurn(prompt, (event: SseEvent) => {
        switch (event.type) {
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
                  ? {
                      ...prev,
                      assistantText: prev.assistantText + data.text,
                    }
                  : null,
              )
            }
            // done, log, tool_output_delta — no UI action needed
            break
          }
          case "turn_completed": {
            setTurns((prev) => [...prev, event.data])
            setStreamingTurn(null)
            setChatState("idle")
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

      controllerRef.current = controller
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
