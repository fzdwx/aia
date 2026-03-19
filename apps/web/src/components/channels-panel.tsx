import { useEffect, useMemo, useState, type FormEvent } from "react"
import { ArrowLeft, Pencil, Plus, Trash2 } from "lucide-react"

import { listSupportedChannels } from "@/lib/api"
import type {
  ChannelListItem,
  ChannelTransport,
  SupportedChannelDefinition,
} from "@/lib/types"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"

type ChannelSchemaProperty = Record<string, unknown>

type ChannelFormState = {
  id: string
  name: string
  transport: ChannelTransport
  enabled: boolean
  config: Record<string, unknown>
}

function cloneValue(value: unknown): unknown {
  if (typeof structuredClone === "function") return structuredClone(value)
  return JSON.parse(JSON.stringify(value)) as unknown
}

function schemaProperties(
  definition: SupportedChannelDefinition | null
): Array<[string, ChannelSchemaProperty]> {
  const properties = definition?.config_schema.properties
  if (!properties || typeof properties !== "object") return []
  return Object.entries(properties).map(([key, value]) => [
    key,
    typeof value === "object" && value ? (value as ChannelSchemaProperty) : {},
  ])
}

function requiredFieldKeys(
  definition: SupportedChannelDefinition | null
): Set<string> {
  const required = definition?.config_schema.required
  if (!Array.isArray(required)) return new Set()
  return new Set(
    required.filter((value): value is string => typeof value === "string")
  )
}

function fieldKind(
  schema: ChannelSchemaProperty
): "text" | "secret" | "boolean" | "url" {
  if (schema["x-secret"] === true) return "secret"
  if (schema.type === "boolean") return "boolean"
  if (schema.format === "uri") return "url"
  return "text"
}

function fieldLabel(key: string, schema: ChannelSchemaProperty): string {
  const label = schema["x-label"]
  if (typeof label === "string" && label.trim().length > 0) return label
  return key.replaceAll("_", " ")
}

function definitionDefaults(
  definition: SupportedChannelDefinition | null
): Record<string, unknown> {
  return Object.fromEntries(
    schemaProperties(definition).map(([key, schema]) => [
      key,
      cloneValue(
        schema.default ?? (fieldKind(schema) === "boolean" ? false : "")
      ),
    ])
  )
}

function normalizeConfigForForm(
  definition: SupportedChannelDefinition | null,
  config: Record<string, unknown>
): Record<string, unknown> {
  const merged = definitionDefaults(definition)
  for (const [key, schema] of schemaProperties(definition)) {
    if (fieldKind(schema) === "boolean") {
      merged[key] = config[key] === true
      continue
    }
    merged[key] = typeof config[key] === "string" ? config[key] : ""
  }
  return merged
}

function emptyChannelForm(
  definition: SupportedChannelDefinition | null
): ChannelFormState {
  return {
    id: "",
    name: "",
    transport: definition?.transport ?? "",
    enabled: true,
    config: definitionDefaults(definition),
  }
}

function isMissingRequiredValue(
  key: string,
  schema: ChannelSchemaProperty,
  value: unknown,
  editing: boolean,
  requiredKeys: Set<string>
): boolean {
  if (!requiredKeys.has(key)) return false
  if (editing && fieldKind(schema) === "secret") return false
  if (fieldKind(schema) === "boolean") return false
  return typeof value !== "string" || value.trim().length === 0
}

function fieldPreview(
  key: string,
  schema: ChannelSchemaProperty,
  channel: ChannelListItem
): string | null {
  const label = fieldLabel(key, schema)
  if (fieldKind(schema) === "secret") {
    return channel.secret_fields_set.includes(key) ? `${label}: set` : null
  }
  const value = channel.config[key]
  if (fieldKind(schema) === "boolean") {
    return `${label}: ${value === true ? "on" : "off"}`
  }
  if (typeof value === "string" && value.trim().length > 0) {
    return `${label}: ${value}`
  }
  return null
}

export function ChannelsPanel() {
  const channelList = useChatStore((s) => s.channelList)
  const setView = useChatStore((s) => s.setView)
  const refreshChannels = useChatStore((s) => s.refreshChannels)
  const storeCreateChannel = useChatStore((s) => s.createChannel)
  const storeUpdateChannel = useChatStore((s) => s.updateChannel)
  const storeDeleteChannel = useChatStore((s) => s.deleteChannel)

  const [catalog, setCatalog] = useState<SupportedChannelDefinition[]>([])
  const [loadingCatalog, setLoadingCatalog] = useState(true)
  const [form, setForm] = useState<ChannelFormState>(emptyChannelForm(null))
  const [submitting, setSubmitting] = useState(false)
  const [editing, setEditing] = useState<string | null>(null)
  const [formOpen, setFormOpen] = useState(channelList.length === 0)

  const selectedDefinition = useMemo(
    () => catalog.find((item) => item.transport === form.transport) ?? null,
    [catalog, form.transport]
  )
  const selectedProperties = useMemo(
    () => schemaProperties(selectedDefinition),
    [selectedDefinition]
  )
  const selectedRequired = useMemo(
    () => requiredFieldKeys(selectedDefinition),
    [selectedDefinition]
  )

  useEffect(() => {
    void refreshChannels().catch(() => {})
    void (async () => {
      try {
        const definitions = await listSupportedChannels()
        setCatalog(definitions)
        setForm((prev) => {
          if (prev.transport || editing) return prev
          return emptyChannelForm(definitions[0] ?? null)
        })
      } finally {
        setLoadingCatalog(false)
      }
    })()
  }, [editing, refreshChannels])

  useEffect(() => {
    if (channelList.length === 0 && !editing) {
      setFormOpen(true)
    }
  }, [channelList.length, editing])

  function updateForm(patch: Partial<ChannelFormState>) {
    setForm((prev) => ({ ...prev, ...patch }))
  }

  function updateConfigField(key: string, value: unknown) {
    setForm((prev) => ({
      ...prev,
      config: {
        ...prev.config,
        [key]: value,
      },
    }))
  }

  function resetForm(nextTransport?: string) {
    const definition =
      catalog.find((item) => item.transport === nextTransport) ??
      catalog[0] ??
      null
    setForm(emptyChannelForm(definition))
    setEditing(null)
  }

  function startEdit(channelId: string) {
    const channel = channelList.find((item) => item.id === channelId)
    if (!channel) return
    const definition =
      catalog.find((item) => item.transport === channel.transport) ?? null
    setForm({
      id: channel.id,
      name: channel.name,
      transport: channel.transport,
      enabled: channel.enabled,
      config: normalizeConfigForForm(definition, channel.config),
    })
    setEditing(channelId)
    setFormOpen(true)
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!selectedDefinition) return
    setSubmitting(true)

    try {
      if (editing) {
        await storeUpdateChannel(editing, {
          name: form.name.trim(),
          enabled: form.enabled,
          config: form.config,
        })
      } else {
        await storeCreateChannel({
          id: form.id.trim(),
          name: form.name.trim(),
          transport: form.transport,
          enabled: form.enabled,
          config: form.config,
        })
      }
      resetForm(form.transport)
      setFormOpen(false)
    } finally {
      setSubmitting(false)
    }
  }

  async function handleDelete(channelId: string) {
    await storeDeleteChannel(channelId)
    if (editing === channelId) resetForm()
  }

  const canSubmit =
    form.id.trim() &&
    form.name.trim() &&
    form.transport &&
    selectedDefinition &&
    selectedProperties.every(([key, schema]) => {
      return !isMissingRequiredValue(
        key,
        schema,
        form.config[key],
        Boolean(editing),
        selectedRequired
      )
    })

  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="mx-auto max-w-[800px] px-6 py-8">
        <div className="mb-8 flex items-center gap-3">
          <button
            onClick={() => setView("chat")}
            className="flex size-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
          >
            <ArrowLeft className="size-4" />
          </button>
          <h1 className="text-lg font-semibold">Channels</h1>
        </div>

        <section className="mb-8">
          <h2 className="mb-3 text-[13px] font-medium text-muted-foreground">
            Configured channels
          </h2>
          {channelList.length === 0 ? (
            <p className="text-[13px] text-muted-foreground/60">
              No channels configured yet.
            </p>
          ) : (
            <div className="space-y-2">
              {channelList.map((channel) => {
                const definition =
                  catalog.find(
                    (item) => item.transport === channel.transport
                  ) ?? null
                const previews = definition
                  ? schemaProperties(definition)
                      .map(([key, schema]) =>
                        fieldPreview(key, schema, channel)
                      )
                      .filter((value): value is string => Boolean(value))
                  : []
                return (
                  <Card
                    key={channel.id}
                    className={cn(
                      "flex items-center justify-between px-4 py-3",
                      channel.enabled && "border-foreground/20"
                    )}
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="text-[13px] font-medium">
                          {channel.name}
                        </span>
                        <Badge variant="secondary" className="text-[10px]">
                          {definition?.label ?? channel.transport}
                        </Badge>
                        <Badge variant="outline" className="text-[10px]">
                          {channel.enabled ? "enabled" : "disabled"}
                        </Badge>
                      </div>
                      <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                        {channel.id}
                      </p>
                      <div className="mt-1 flex flex-wrap gap-1.5">
                        {previews.map((preview) => (
                          <Badge
                            key={`${channel.id}-${preview}`}
                            variant="outline"
                            className="text-[10px] font-normal"
                          >
                            {preview}
                          </Badge>
                        ))}
                      </div>
                    </div>
                    <div className="ml-3 flex shrink-0 gap-1">
                      <button
                        onClick={() => startEdit(channel.id)}
                        className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
                      >
                        <Pencil className="size-3.5" />
                      </button>
                      <button
                        onClick={() => void handleDelete(channel.id)}
                        className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                      >
                        <Trash2 className="size-3.5" />
                      </button>
                    </div>
                  </Card>
                )
              })}
            </div>
          )}
        </section>

        <Separator className="mb-8 opacity-30" />

        <section>
          <div className="mb-3 flex items-center justify-between">
            <h2 className="text-[13px] font-medium text-muted-foreground">
              {editing ? `Edit Channel — ${editing}` : "Add Channel"}
            </h2>
            {!formOpen ? (
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
            ) : editing ? (
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
            ) : null}
          </div>

          {formOpen ? (
            <Card className="p-4">
              <form onSubmit={handleSubmit} className="space-y-4">
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <div>
                    <label className="mb-1 block text-[12px] text-muted-foreground">
                      ID
                    </label>
                    <Input
                      value={form.id}
                      onChange={(event) =>
                        updateForm({ id: event.target.value })
                      }
                      placeholder="e.g. feishu-main"
                      className="h-8 text-[13px]"
                      disabled={!!editing}
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-[12px] text-muted-foreground">
                      Name
                    </label>
                    <Input
                      value={form.name}
                      onChange={(event) =>
                        updateForm({ name: event.target.value })
                      }
                      placeholder="e.g. Main workspace"
                      className="h-8 text-[13px]"
                    />
                  </div>
                </div>

                <div>
                  <label className="mb-1 block text-[12px] text-muted-foreground">
                    Transport
                  </label>
                  <select
                    value={form.transport}
                    disabled={!!editing || loadingCatalog}
                    onChange={(event) => {
                      const next =
                        catalog.find(
                          (item) => item.transport === event.target.value
                        ) ?? null
                      setForm((prev) => ({
                        ...prev,
                        transport: event.target.value,
                        config: definitionDefaults(next),
                      }))
                    }}
                    className="flex h-8 w-full items-center rounded-lg border border-input bg-transparent px-2.5 text-[13px] text-foreground"
                  >
                    {catalog.map((definition) => (
                      <option
                        key={definition.transport}
                        value={definition.transport}
                      >
                        {definition.label}
                      </option>
                    ))}
                  </select>
                </div>

                {selectedDefinition ? (
                  <div className="space-y-3">
                    {selectedDefinition.description ? (
                      <p className="text-[12px] text-muted-foreground">
                        {selectedDefinition.description}
                      </p>
                    ) : null}

                    {selectedProperties.map(([key, schema]) => {
                      const kind = fieldKind(schema)
                      const label = fieldLabel(key, schema)
                      const description =
                        typeof schema.description === "string"
                          ? schema.description
                          : null
                      const value = form.config[key]

                      if (kind === "boolean") {
                        return (
                          <label
                            key={key}
                            className="flex items-center justify-between gap-3 rounded-lg border border-border/30 bg-muted/20 px-3 py-2 text-[12px] text-foreground"
                          >
                            <span>{label}</span>
                            <Switch
                              checked={value === true}
                              onCheckedChange={(checked: boolean) =>
                                updateConfigField(key, checked)
                              }
                            />
                          </label>
                        )
                      }

                      return (
                        <div key={key}>
                          <label className="mb-1 block text-[12px] text-muted-foreground">
                            {label}
                            {editing && kind === "secret"
                              ? " (leave blank to keep existing)"
                              : ""}
                          </label>
                          <Input
                            type={
                              kind === "secret"
                                ? "password"
                                : kind === "url"
                                  ? "url"
                                  : "text"
                            }
                            value={typeof value === "string" ? value : ""}
                            onChange={(event) =>
                              updateConfigField(key, event.target.value)
                            }
                            placeholder={
                              typeof schema.default === "string"
                                ? schema.default
                                : undefined
                            }
                            className="h-8 text-[13px]"
                          />
                          {description ? (
                            <p className="mt-1 text-[11px] text-muted-foreground/70">
                              {description}
                            </p>
                          ) : null}
                        </div>
                      )
                    })}
                  </div>
                ) : (
                  <p className="text-[12px] text-muted-foreground">
                    No supported channel definitions available.
                  </p>
                )}

                <label className="flex items-center justify-between gap-3 rounded-lg border border-border/30 bg-muted/20 px-3 py-2 text-[12px] text-foreground">
                  <span>Enabled</span>
                  <Switch
                    checked={form.enabled}
                    onCheckedChange={(checked: boolean) =>
                      updateForm({ enabled: checked })
                    }
                  />
                </label>

                <Button
                  type="submit"
                  size="sm"
                  disabled={submitting || !canSubmit}
                  className="mt-2 w-full"
                >
                  <Plus className="mr-1.5 size-3.5" />
                  {editing ? "Update Channel" : "Add Channel"}
                </Button>
              </form>
            </Card>
          ) : null}
        </section>
      </div>
    </ScrollArea>
  )
}
