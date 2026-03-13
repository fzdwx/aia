import { useState } from "react"
import { ArrowLeft, Pencil, Plus, Trash2, X } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"
import type { ModelConfig } from "@/lib/types"

type ModelFormRow = {
  id: string
  display_name: string
  supports_reasoning: boolean
  reasoning_effort: string
}

function emptyModelRow(): ModelFormRow {
  return { id: "", display_name: "", supports_reasoning: false, reasoning_effort: "medium" }
}

export function SettingsPanel() {
  const providerList = useChatStore((s) => s.providerList)
  const setView = useChatStore((s) => s.setView)
  const storeCreateProvider = useChatStore((s) => s.createProvider)
  const storeUpdateProvider = useChatStore((s) => s.updateProvider)
  const storeDeleteProvider = useChatStore((s) => s.deleteProvider)

  const [name, setName] = useState("")
  const [kind, setKind] = useState("openai-responses")
  const [apiKey, setApiKey] = useState("")
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1")
  const [models, setModels] = useState<ModelFormRow[]>([emptyModelRow()])
  const [submitting, setSubmitting] = useState(false)
  const [editing, setEditing] = useState<string | null>(null)
  const [formOpen, setFormOpen] = useState(providerList.length === 0)

  function resetForm() {
    setName("")
    setKind("openai-responses")
    setApiKey("")
    setBaseUrl("https://api.openai.com/v1")
    setModels([emptyModelRow()])
    setEditing(null)
  }

  function startEdit(providerName: string) {
    const p = providerList.find((x) => x.name === providerName)
    if (!p) return
    setName(p.name)
    setKind(p.kind)
    setApiKey("")
    setBaseUrl(p.base_url)
    setModels(
      p.models.map((m) => ({
        id: m.id,
        display_name: m.display_name ?? "",
        supports_reasoning: m.supports_reasoning,
        reasoning_effort: m.reasoning_effort ?? "medium",
      })),
    )
    setEditing(providerName)
    setFormOpen(true)
  }

  function updateModelRow(index: number, patch: Partial<ModelFormRow>) {
    setModels((prev) =>
      prev.map((row, i) => (i === index ? { ...row, ...patch } : row)),
    )
  }

  function removeModelRow(index: number) {
    setModels((prev) => prev.filter((_, i) => i !== index))
  }

  function buildModels(): ModelConfig[] {
    return models
      .filter((m) => m.id.trim())
      .map((m) => ({
        id: m.id.trim(),
        display_name: m.display_name.trim() || null,
        context_window: null,
        default_temperature: null,
        supports_reasoning: m.supports_reasoning,
        reasoning_effort: m.supports_reasoning ? m.reasoning_effort : null,
      }))
  }

  const hasValidModel = models.some((m) => m.id.trim())

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!hasValidModel) return
    setSubmitting(true)
    try {
      if (editing) {
        const body: Record<string, unknown> = {
          kind,
          models: buildModels(),
          active_model: buildModels()[0]?.id,
          base_url: baseUrl.trim(),
        }
        if (apiKey.trim()) body.api_key = apiKey.trim()
        await storeUpdateProvider(editing, body as Parameters<typeof storeUpdateProvider>[1])
      } else {
        await storeCreateProvider({
          name: name.trim(),
          kind,
          models: buildModels(),
          active_model: buildModels()[0]?.id,
          api_key: apiKey.trim(),
          base_url: baseUrl.trim(),
        })
      }
      resetForm()
      setFormOpen(false)
    } finally {
      setSubmitting(false)
    }
  }

  async function handleDelete(providerName: string) {
    await storeDeleteProvider(providerName)
    if (editing === providerName) {
      resetForm()
    }
  }

  return (
    <ScrollArea className="flex-1">
      <div className="mx-auto max-w-[800px] px-6 py-8">
        {/* Header */}
        <div className="mb-8 flex items-center gap-3">
          <button
            onClick={() => setView("chat")}
            className="flex size-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
          >
            <ArrowLeft className="size-4" />
          </button>
          <h1 className="text-lg font-semibold">Settings</h1>
        </div>

        {/* Provider list */}
        <section className="mb-8">
          <h2 className="mb-3 text-[13px] font-medium text-muted-foreground">
            Providers
          </h2>
          {providerList.length === 0 ? (
            <p className="text-[13px] text-muted-foreground/60">
              No providers configured yet.
            </p>
          ) : (
            <div className="space-y-2">
              {providerList.map((p) => (
                <Card
                  key={p.name}
                  className={cn(
                    "flex items-center justify-between px-4 py-3",
                    p.active && "border-foreground/20",
                  )}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-[13px] font-medium">{p.name}</span>
                      <Badge variant="secondary" className="text-[10px]">
                        {p.kind}
                      </Badge>
                      {p.active && (
                        <Badge variant="secondary" className="text-[10px]">
                          active
                        </Badge>
                      )}
                    </div>
                    <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                      {p.base_url}
                    </p>
                    <div className="mt-1 flex flex-wrap gap-1.5">
                      {p.models.map((m) => (
                        <Badge
                          key={m.id}
                          variant="outline"
                          className="text-[10px] font-normal"
                        >
                          {m.display_name ?? m.id}
                        </Badge>
                      ))}
                    </div>
                  </div>
                  <div className="ml-3 flex shrink-0 gap-1">
                    <button
                      onClick={() => startEdit(p.name)}
                      className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
                    >
                      <Pencil className="size-3.5" />
                    </button>
                    <button
                      onClick={() => handleDelete(p.name)}
                      className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                    >
                      <Trash2 className="size-3.5" />
                    </button>
                  </div>
                </Card>
              ))}
            </div>
          )}
        </section>

        <Separator className="mb-8 opacity-30" />

        {/* Add / Edit provider form */}
        <section>
          <div className="mb-3 flex items-center justify-between">
            <h2 className="text-[13px] font-medium text-muted-foreground">
              {editing ? `Edit Provider — ${editing}` : "Add Provider"}
            </h2>
            {!formOpen && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  resetForm()
                  setFormOpen(true)
                }}
              >
                <Plus className="mr-1.5 size-3.5" />
                Add
              </Button>
            )}
            {formOpen && editing && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  resetForm()
                  setFormOpen(false)
                }}
              >
                Cancel
              </Button>
            )}
          </div>

          {formOpen && (
            <Card className="p-4">
              <form onSubmit={handleSubmit} className="space-y-4">
                {/* Name + Protocol row */}
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <div>
                    <label className="mb-1 block text-[12px] text-muted-foreground">
                      Name
                    </label>
                    <Input
                      value={name}
                      onChange={(e) => setName(e.target.value)}
                      placeholder="e.g. openai-main"
                      className="h-8 text-[13px]"
                      disabled={!!editing}
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-[12px] text-muted-foreground">
                      Protocol
                    </label>
                    <Select value={kind} onValueChange={setKind}>
                      <SelectTrigger className="h-8 w-full text-[13px]">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="openai-responses">
                          OpenAI Responses
                        </SelectItem>
                        <SelectItem value="openai-chat-completions">
                          OpenAI Chat Completions
                        </SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                </div>

                {/* API Key */}
                <div>
                  <label className="mb-1 block text-[12px] text-muted-foreground">
                    API Key{editing && " (leave blank to keep existing)"}
                  </label>
                  <Input
                    type="password"
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder="sk-..."
                    className="h-8 text-[13px]"
                  />
                </div>

                {/* Base URL */}
                <div>
                  <label className="mb-1 block text-[12px] text-muted-foreground">
                    Base URL
                  </label>
                  <Input
                    value={baseUrl}
                    onChange={(e) => setBaseUrl(e.target.value)}
                    className="h-8 text-[13px]"
                  />
                </div>

                <Separator className="opacity-30" />

                {/* Models */}
                <div>
                  <label className="mb-2 block text-[12px] font-medium text-muted-foreground">
                    Models
                  </label>
                  <div className="space-y-3">
                    {models.map((row, i) => (
                      <div
                        key={i}
                        className="rounded-lg border border-border/30 bg-muted/20 p-3"
                      >
                        <div className="flex items-start gap-2">
                          <div className="grid flex-1 grid-cols-1 gap-2 sm:grid-cols-2">
                            <Input
                              value={row.id}
                              onChange={(e) =>
                                updateModelRow(i, { id: e.target.value })
                              }
                              placeholder="Model ID (e.g. gpt-4.1-mini)"
                              className="h-7 text-[12px]"
                            />
                            <Input
                              value={row.display_name}
                              onChange={(e) =>
                                updateModelRow(i, {
                                  display_name: e.target.value,
                                })
                              }
                              placeholder="Display Name (optional)"
                              className="h-7 text-[12px]"
                            />
                          </div>
                          {models.length > 1 && (
                            <button
                              type="button"
                              onClick={() => removeModelRow(i)}
                              className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                            >
                              <X className="size-3.5" />
                            </button>
                          )}
                        </div>
                        {/* Reasoning toggle */}
                        <div className="mt-2 flex items-center gap-3">
                          <label className="flex items-center gap-2 text-[11px] text-muted-foreground">
                            <Switch
                              checked={row.supports_reasoning}
                              onCheckedChange={(checked: boolean) =>
                                updateModelRow(i, {
                                  supports_reasoning: checked,
                                })
                              }
                            />
                            Reasoning
                          </label>
                          {row.supports_reasoning && (
                            <Select
                              value={row.reasoning_effort}
                              onValueChange={(v: string) =>
                                updateModelRow(i, { reasoning_effort: v })
                              }
                            >
                              <SelectTrigger className="h-6 w-[100px] text-[11px]" size="sm">
                                <SelectValue />
                              </SelectTrigger>
                              <SelectContent>
                                <SelectItem value="low">Low</SelectItem>
                                <SelectItem value="medium">Medium</SelectItem>
                                <SelectItem value="high">High</SelectItem>
                              </SelectContent>
                            </Select>
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setModels((prev) => [...prev, emptyModelRow()])}
                    className="mt-2"
                  >
                    <Plus className="mr-1.5 size-3" />
                    Add Model
                  </Button>
                </div>

                <Button
                  type="submit"
                  size="sm"
                  disabled={
                    submitting ||
                    (!editing && !name.trim()) ||
                    !hasValidModel ||
                    (!editing && !apiKey.trim())
                  }
                  className="mt-2 w-full"
                >
                  <Plus className="mr-1.5 size-3.5" />
                  {editing ? "Update Provider" : "Add Provider"}
                </Button>
              </form>
            </Card>
          )}
        </section>
      </div>
    </ScrollArea>
  )
}
