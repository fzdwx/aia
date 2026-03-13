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

export async function submitTurn(prompt: string): Promise<void> {
  const res = await fetch("/api/turn", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ prompt }),
  })
  if (!res.ok) throw new Error(`POST /api/turn failed: ${res.status}`)
}

/**
 * Connect to the global SSE stream. Returns a cleanup function.
 */
export function connectEvents(onEvent: (event: SseEvent) => void): () => void {
  const es = new EventSource("/api/events")

  function handle(type: SseEvent["type"]) {
    return (e: MessageEvent) => {
      try {
        const data = JSON.parse(e.data as string) as Record<string, unknown>
        onEvent({ type, data } as SseEvent)
      } catch {
        // skip malformed
      }
    }
  }

  es.addEventListener("stream", handle("stream"))
  es.addEventListener("status", handle("status"))
  es.addEventListener("turn_completed", handle("turn_completed"))
  es.addEventListener("error", handle("error"))

  return () => es.close()
}
