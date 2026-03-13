import type { ModelConfig, ProviderInfo, ProviderListItem, SseEvent, TurnLifecycle } from "./types"

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

export async function listProviders(): Promise<ProviderListItem[]> {
  const res = await fetch("/api/providers/list")
  if (!res.ok) throw new Error(`GET /api/providers/list failed: ${res.status}`)
  return res.json() as Promise<ProviderListItem[]>
}

export async function createProvider(body: {
  name: string
  kind: string
  models: ModelConfig[]
  active_model?: string
  api_key: string
  base_url: string
}): Promise<void> {
  const res = await fetch("/api/providers", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`POST /api/providers failed: ${res.status}`)
}

export async function updateProvider(
  name: string,
  body: {
    kind?: string
    models?: ModelConfig[]
    active_model?: string
    api_key?: string
    base_url?: string
  },
): Promise<void> {
  const res = await fetch(`/api/providers/${encodeURIComponent(name)}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`PUT /api/providers/${name} failed: ${res.status}`)
}

export async function deleteProvider(name: string): Promise<void> {
  const res = await fetch(`/api/providers/${encodeURIComponent(name)}`, {
    method: "DELETE",
  })
  if (!res.ok) throw new Error(`DELETE /api/providers/${name} failed: ${res.status}`)
}

export async function switchProvider(name: string, modelId?: string): Promise<ProviderInfo> {
  const res = await fetch("/api/providers/switch", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name, model_id: modelId }),
  })
  if (!res.ok)
    throw new Error(`POST /api/providers/switch failed: ${res.status}`)
  return res.json() as Promise<ProviderInfo>
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
