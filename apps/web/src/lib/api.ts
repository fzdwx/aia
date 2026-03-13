import type { ProviderInfo, SseEvent, TurnLifecycle } from "./types"

export async function fetchProviders(): Promise<ProviderInfo> {
  const res = await fetch("/api/providers")
  if (!res.ok) throw new Error(`GET /api/providers failed: ${res.status}`)
  return res.json() as Promise<ProviderInfo>
}

export async function fetchHistory(): Promise<TurnLifecycle[]> {
  const res = await fetch("/api/session/history")
  if (!res.ok)
    throw new Error(`GET /api/session/history failed: ${res.status}`)
  return res.json() as Promise<TurnLifecycle[]>
}

export function submitTurn(
  prompt: string,
  onEvent: (event: SseEvent) => void,
): AbortController {
  const controller = new AbortController()

  fetch("/api/turn", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ prompt }),
    signal: controller.signal,
  })
    .then((res) => {
      if (!res.ok || !res.body) {
        onEvent({
          type: "error",
          data: { message: `POST /api/turn failed: ${res.status}` },
        })
        return
      }
      readSseStream(res.body, onEvent)
    })
    .catch((err: unknown) => {
      if (err instanceof DOMException && err.name === "AbortError") return
      onEvent({
        type: "error",
        data: {
          message: err instanceof Error ? err.message : "Network error",
        },
      })
    })

  return controller
}

async function readSseStream(
  body: ReadableStream<Uint8Array>,
  onEvent: (event: SseEvent) => void,
): Promise<void> {
  const reader = body.getReader()
  const decoder = new TextDecoder()
  let buffer = ""
  let currentEvent = ""
  let currentData = ""

  for (;;) {
    const { done, value } = await reader.read()
    if (done) break

    buffer += decoder.decode(value, { stream: true })
    const lines = buffer.split("\n")
    buffer = lines.pop() ?? ""

    for (const line of lines) {
      if (line.startsWith("event: ")) {
        currentEvent = line.slice(7).trim()
      } else if (line.startsWith("data: ")) {
        currentData = line.slice(6)
      } else if (line === "") {
        // Empty line = end of SSE message
        if (currentEvent && currentData) {
          try {
            const parsed = JSON.parse(currentData) as Record<string, unknown>
            onEvent({
              type: currentEvent as SseEvent["type"],
              data: parsed,
            } as SseEvent)
          } catch {
            // Skip malformed events
          }
        }
        currentEvent = ""
        currentData = ""
      }
    }
  }
}
