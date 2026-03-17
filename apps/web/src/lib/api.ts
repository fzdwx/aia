import type {
  CurrentTurnSnapshot,
  HistoryPage,
  ModelConfig,
  ProviderInfo,
  ProviderListItem,
  SessionListItem,
  SseEvent,
  TraceListPage,
  TraceRecord,
  TraceSummary,
} from "./types"

export type ContextStats = {
  total_entries: number
  anchor_count: number
  entries_since_last_anchor: number
  last_input_tokens: number | null
  context_limit: number | null
  output_limit: number | null
  pressure_ratio: number | null
}

// ── Session management ─────────────────────────────────────────

export async function fetchSessions(): Promise<SessionListItem[]> {
  const res = await fetch("/api/sessions")
  if (!res.ok) throw new Error(`GET /api/sessions failed: ${res.status}`)
  return res.json() as Promise<SessionListItem[]>
}

export async function createSession(title?: string): Promise<SessionListItem> {
  const res = await fetch("/api/sessions", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ title }),
  })
  if (!res.ok) throw new Error(`POST /api/sessions failed: ${res.status}`)
  return res.json() as Promise<SessionListItem>
}

export async function deleteSession(id: string): Promise<void> {
  const res = await fetch(`/api/sessions/${encodeURIComponent(id)}`, {
    method: "DELETE",
  })
  if (!res.ok)
    throw new Error(`DELETE /api/sessions/${id} failed: ${res.status}`)
}

// ── Session-scoped endpoints ───────────────────────────────────

export async function fetchSessionInfo(
  sessionId?: string
): Promise<ContextStats> {
  const params = sessionId ? `?session_id=${encodeURIComponent(sessionId)}` : ""
  const res = await fetch(`/api/session/info${params}`)
  if (!res.ok) throw new Error(`GET /api/session/info failed: ${res.status}`)
  return res.json() as Promise<ContextStats>
}

export async function fetchHistory(params?: {
  sessionId?: string
  beforeTurnId?: string
  limit?: number
  signal?: AbortSignal
}): Promise<HistoryPage> {
  const search = new URLSearchParams()
  if (params?.sessionId) search.set("session_id", params.sessionId)
  if (params?.beforeTurnId) search.set("before_turn_id", params.beforeTurnId)
  if (params?.limit != null) search.set("limit", String(params.limit))
  const query = search.size > 0 ? `?${search.toString()}` : ""
  const res = await fetch(`/api/session/history${query}`, {
    signal: params?.signal,
  })
  if (!res.ok) throw new Error(`GET /api/session/history failed: ${res.status}`)
  return (await res.json()) as Promise<HistoryPage>
}

export async function fetchCurrentTurn(
  sessionId?: string
): Promise<CurrentTurnSnapshot | null> {
  const params = sessionId ? `?session_id=${encodeURIComponent(sessionId)}` : ""
  const res = await fetch(`/api/session/current-turn${params}`)
  if (!res.ok)
    throw new Error(`GET /api/session/current-turn failed: ${res.status}`)
  return (await res.json()) as Promise<CurrentTurnSnapshot | null>
}

export async function submitTurn(
  prompt: string,
  sessionId?: string
): Promise<void> {
  const res = await fetch("/api/turn", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ prompt, session_id: sessionId }),
  })
  if (!res.ok) throw new Error(`POST /api/turn failed: ${res.status}`)
}

export async function cancelTurn(sessionId?: string): Promise<boolean> {
  const res = await fetch("/api/turn/cancel", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ session_id: sessionId }),
  })
  if (!res.ok) throw new Error(`POST /api/turn/cancel failed: ${res.status}`)
  const payload = (await res.json()) as { cancelled?: boolean }
  return payload.cancelled === true
}

// ── Provider endpoints (unchanged) ─────────────────────────────

export async function fetchProviders(): Promise<ProviderInfo> {
  const res = await fetch("/api/providers")
  if (!res.ok) throw new Error(`GET /api/providers failed: ${res.status}`)
  return (await res.json()) as Promise<ProviderInfo>
}

export async function listProviders(): Promise<ProviderListItem[]> {
  const res = await fetch("/api/providers/list")
  if (!res.ok) throw new Error(`GET /api/providers/list failed: ${res.status}`)
  return (await res.json()) as Promise<ProviderListItem[]>
}

export async function fetchTraces(params?: {
  page?: number
  page_size?: number
}): Promise<TraceListPage> {
  const search = new URLSearchParams()
  if (params?.page != null) search.set("page", String(params.page))
  if (params?.page_size != null)
    search.set("page_size", String(params.page_size))
  const query = search.size > 0 ? `?${search.toString()}` : ""
  const res = await fetch(`/api/traces${query}`)
  if (!res.ok) throw new Error(`GET /api/traces failed: ${res.status}`)
  return (await res.json()) as Promise<TraceListPage>
}

export async function fetchTrace(id: string): Promise<TraceRecord> {
  const res = await fetch(`/api/traces/${encodeURIComponent(id)}`)
  if (!res.ok) throw new Error(`GET /api/traces/${id} failed: ${res.status}`)
  return (await res.json()) as Promise<TraceRecord>
}

export async function fetchTraceSummary(): Promise<TraceSummary> {
  const res = await fetch("/api/traces/summary")
  if (!res.ok) throw new Error(`GET /api/traces/summary failed: ${res.status}`)
  return (await res.json()) as Promise<TraceSummary>
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
  }
): Promise<void> {
  const res = await fetch(`/api/providers/${encodeURIComponent(name)}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  })
  if (!res.ok)
    throw new Error(`PUT /api/providers/${name} failed: ${res.status}`)
}

export async function deleteProvider(name: string): Promise<void> {
  const res = await fetch(`/api/providers/${encodeURIComponent(name)}`, {
    method: "DELETE",
  })
  if (!res.ok)
    throw new Error(`DELETE /api/providers/${name} failed: ${res.status}`)
}

export async function switchProvider(
  name: string,
  modelId?: string
): Promise<ProviderInfo> {
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
  es.addEventListener("context_compressed", handle("context_compressed"))
  es.addEventListener("sync_required", handle("sync_required"))
  es.addEventListener("error", handle("error"))
  es.addEventListener("session_created", handle("session_created"))
  es.addEventListener("session_deleted", handle("session_deleted"))
  es.addEventListener("turn_cancelled", handle("turn_cancelled"))

  return () => es.close()
}
