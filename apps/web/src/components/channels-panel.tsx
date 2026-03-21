import { useEffect, useMemo, useState, type FormEvent } from "react"
import { ArrowLeft, Trash2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import type {
  ChannelListItem,
  ChannelTransport,
  SupportedChannelDefinition,
} from "@/lib/types"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Switch } from "@/components/ui/switch"
import { useChannelsStore } from "@/stores/channels-store"
import { useChatStore } from "@/stores/chat-store"

type ChannelSchemaProperty = Record<string, unknown>

type ChannelFormState = {
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

function configuredChannelsForTransport(
  configuredChannels: ChannelListItem[],
  transport: ChannelTransport | null
): ChannelListItem[] {
  if (!transport) return []
  return configuredChannels.filter((channel) => channel.transport === transport)
}

export function ChannelsPanel({ embedded = false }: { embedded?: boolean }) {
  const setView = useChatStore((s) => s.setView)
  const initializeChannels = useChannelsStore((s) => s.initialize)
  const supportedChannels = useChannelsStore((s) => s.supportedChannels)
  const configuredChannels = useChannelsStore((s) => s.configuredChannels)
  const selectedTransport = useChannelsStore((s) => s.selectedTransport)
  const channelsLoading = useChannelsStore((s) => s.loading)
  const channelsError = useChannelsStore((s) => s.error)
  const createChannel = useChannelsStore((s) => s.createChannel)
  const updateChannel = useChannelsStore((s) => s.updateChannel)
  const deleteChannel = useChannelsStore((s) => s.deleteChannel)

  const [form, setForm] = useState<ChannelFormState>(emptyChannelForm(null))
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    void initializeChannels().catch(() => {})
  }, [initializeChannels])

  const selectedDefinition = useMemo(() => {
    if (selectedTransport) {
      const selected =
        supportedChannels.find(
          (channel) => channel.transport === selectedTransport
        ) ?? null
      if (selected) return selected
    }
    return supportedChannels[0] ?? null
  }, [selectedTransport, supportedChannels])

  const matchingChannels = useMemo(
    () =>
      configuredChannelsForTransport(
        configuredChannels,
        selectedDefinition?.transport ?? null
      ),
    [configuredChannels, selectedDefinition]
  )
  const configuredChannel = matchingChannels[0] ?? null
  const selectedProperties = useMemo(
    () => schemaProperties(selectedDefinition),
    [selectedDefinition]
  )
  const selectedRequired = useMemo(
    () => requiredFieldKeys(selectedDefinition),
    [selectedDefinition]
  )

  useEffect(() => {
    if (!selectedDefinition) return

    if (configuredChannel) {
      setForm({
        enabled: configuredChannel.enabled,
        config: normalizeConfigForForm(
          selectedDefinition,
          configuredChannel.config
        ),
      })
      return
    }

    setForm(emptyChannelForm(selectedDefinition))
  }, [configuredChannel, selectedDefinition])

  function updateConfigField(key: string, value: unknown) {
    setForm((prev) => ({
      ...prev,
      config: {
        ...prev.config,
        [key]: value,
      },
    }))
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!selectedDefinition) return

    setSubmitting(true)

    try {
      if (configuredChannel) {
        await updateChannel(configuredChannel.id, {
          enabled: form.enabled,
          config: form.config,
        })
      } else {
        await createChannel({
          id: selectedDefinition.transport,
          name: selectedDefinition.label,
          transport: selectedDefinition.transport,
          enabled: form.enabled,
          config: form.config,
        })
      }
    } finally {
      setSubmitting(false)
    }
  }

  async function handleDelete() {
    if (!configuredChannel) return
    await deleteChannel(configuredChannel.id)
  }

  const canSubmit =
    Boolean(selectedDefinition) &&
    selectedProperties.every(([key, schema]) => {
      return !isMissingRequiredValue(
        key,
        schema,
        form.config[key],
        Boolean(configuredChannel),
        selectedRequired
      )
    })

  const content = (
    <div
      className={
        embedded
          ? "space-y-3"
          : "mx-auto max-w-[920px] px-4 py-6 sm:px-6 sm:py-8"
      }
    >
      {!embedded ? (
        <div className="mb-6 flex items-start justify-between gap-3">
          <div className="flex items-start gap-3">
            <button
              type="button"
              onClick={() => setView("chat")}
              className="flex size-9 shrink-0 items-center justify-center rounded-xl text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
            >
              <ArrowLeft className="size-4" />
            </button>

            <div>
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="text-lg font-semibold">Channels</h1>
                {selectedDefinition ? (
                  <Badge variant="secondary" className="text-[10px]">
                    {selectedDefinition.transport}
                  </Badge>
                ) : null}
              </div>
              <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
                {selectedDefinition?.description ??
                  "Select a transport from the sidebar, then manage the runtime profile for that channel on the right."}
              </p>
            </div>
          </div>

          {configuredChannel ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="shrink-0 text-destructive hover:bg-destructive/10 hover:text-destructive"
              onClick={() => void handleDelete()}
            >
              <Trash2 className="size-3.5" />
              Delete
            </Button>
          ) : null}
        </div>
      ) : null}

      {!selectedDefinition ? (
        <section
          className={
            embedded
              ? "rounded-xl border border-border/30 bg-card/70 px-4 py-4 shadow-[var(--workspace-shadow)]"
              : "rounded-2xl border border-border/30 bg-card/70 px-5 py-6 shadow-[var(--workspace-shadow)]"
          }
        >
          <p className="text-sm font-medium text-foreground">
            No supported channels available.
          </p>
          <p className="mt-2 text-[12px] leading-6 text-muted-foreground">
            {channelsLoading
              ? "Loading channel catalog..."
              : (channelsError ??
                "The server did not return any supported channel type.")}
          </p>
        </section>
      ) : (
        <form
          onSubmit={handleSubmit}
          className={embedded ? "space-y-3" : "space-y-4"}
        >
          <section
            className={
              embedded
                ? "rounded-xl border border-border/30 bg-card/70 p-4 shadow-[var(--workspace-shadow)]"
                : "rounded-2xl border border-border/30 bg-card/70 p-5 shadow-[var(--workspace-shadow)]"
            }
          >
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <div className="flex flex-wrap items-center gap-2">
                  <h2 className="text-[15px] font-semibold">
                    {selectedDefinition.label}
                  </h2>
                  <Badge variant="secondary" className="text-[10px]">
                    {selectedDefinition.transport}
                  </Badge>
                  <Badge variant="outline" className="text-[10px]">
                    {configuredChannel
                      ? configuredChannel.enabled
                        ? "Enabled"
                        : "Disabled"
                      : "Draft"}
                  </Badge>
                </div>
              </div>

              {embedded && configuredChannel ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="shrink-0 text-destructive hover:bg-destructive/10 hover:text-destructive"
                  onClick={() => void handleDelete()}
                >
                  <Trash2 className="size-3.5" />
                  Delete
                </Button>
              ) : null}
            </div>

            {matchingChannels.length > 1 ? (
              <div className="mt-3 rounded-lg border border-border/30 bg-muted/35 px-3 py-2.5 text-[12px] leading-5 text-foreground/85">
                Multiple saved profiles were found for this transport. This
                panel is currently editing the first configured profile:
                <span className="ml-1 font-medium">
                  {configuredChannel?.id}
                </span>
              </div>
            ) : null}

            {selectedProperties.length > 0 ? (
              <div className="mt-4 grid gap-2.5 md:grid-cols-2">
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
                        className="workspace-panel-soft flex items-start justify-between gap-4 px-3 py-3 md:col-span-2"
                      >
                        <div className="pr-4">
                          <div className="flex flex-wrap items-center gap-2">
                            <p className="text-sm font-medium text-foreground">
                              {label}
                            </p>
                            {selectedRequired.has(key) ? (
                              <Badge variant="outline" className="text-[10px]">
                                required
                              </Badge>
                            ) : null}
                          </div>
                          {description ? (
                            <p className="mt-2 text-[12px] leading-6 text-muted-foreground">
                              {description}
                            </p>
                          ) : null}
                        </div>

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
                    <div key={key} className="workspace-panel-soft px-3 py-3">
                      <div className="flex flex-wrap items-center gap-2">
                        <p className="text-sm font-medium text-foreground">
                          {label}
                        </p>
                        {selectedRequired.has(key) ? (
                          <Badge variant="outline" className="text-[10px]">
                            required
                          </Badge>
                        ) : null}
                        {configuredChannel && kind === "secret" ? (
                          <Badge variant="outline" className="text-[10px]">
                            keep on blank
                          </Badge>
                        ) : null}
                      </div>

                      {description ? (
                        <p className="mt-2 text-[12px] leading-6 text-muted-foreground">
                          {description}
                        </p>
                      ) : null}

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
                        className="mt-3 h-9 text-[13px]"
                      />
                    </div>
                  )
                })}
              </div>
            ) : (
              <p className="mt-3 text-[12px] leading-5 text-muted-foreground">
                This channel type does not expose configurable fields.
              </p>
            )}
          </section>

          <section
            className={
              embedded
                ? "rounded-xl border border-border/30 bg-card/70 p-4 shadow-[var(--workspace-shadow)]"
                : "rounded-2xl border border-border/30 bg-card/70 p-5 shadow-[var(--workspace-shadow)]"
            }
          >
            <label className="workspace-panel-soft flex items-start justify-between gap-4 px-3 py-3">
              <div className="pr-4">
                <p className="text-sm font-medium text-foreground">Enabled</p>
                <p className="mt-2 text-[12px] leading-6 text-muted-foreground">
                  Turn this off to keep the profile stored without running its
                  transport worker.
                </p>
              </div>

              <Switch
                checked={form.enabled}
                onCheckedChange={(checked: boolean) =>
                  setForm((prev) => ({ ...prev, enabled: checked }))
                }
              />
            </label>

            <div className="mt-4 flex flex-col-reverse gap-2.5 border-t border-border/20 pt-4 sm:flex-row sm:items-center sm:justify-between">
              <Button
                type="submit"
                disabled={submitting || !canSubmit}
                className="sm:ml-auto sm:min-w-[180px]"
              >
                {configuredChannel
                  ? "Save Configuration"
                  : "Create Configuration"}
              </Button>
            </div>
          </section>
        </form>
      )}
    </div>
  )

  if (embedded) {
    return content
  }

  return <ScrollArea className="min-h-0 flex-1">{content}</ScrollArea>
}
