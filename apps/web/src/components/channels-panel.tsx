import { useEffect, useMemo, useState, type FormEvent } from "react"
import { ArrowLeft, Trash2 } from "lucide-react"

import type {
  ChannelListItem,
  ChannelTransport,
  SupportedChannelDefinition,
} from "@/lib/types"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
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

export function ChannelsPanel() {
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
        supportedChannels.find((channel) => channel.transport === selectedTransport) ??
        null
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
        config: normalizeConfigForForm(selectedDefinition, configuredChannel.config),
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

  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="mx-auto max-w-[920px] px-4 py-6 sm:px-6 sm:py-8">
        <div className="mb-6 flex items-start gap-3">
          <button
            onClick={() => setView("chat")}
            className="flex size-9 shrink-0 items-center justify-center rounded-xl text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
          >
            <ArrowLeft className="size-4" />
          </button>
          <div>
            <h1 className="text-lg font-semibold">Channels</h1>
            <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
              Select a supported channel type from the sidebar, then configure
              its runtime settings here.
            </p>
          </div>
        </div>

        {!selectedDefinition ? (
          <div className="px-1 py-8">
            <p className="text-sm font-medium">No supported channels available.</p>
            <p className="mt-2 text-[12px] leading-5 text-muted-foreground">
              {channelsLoading
                ? "Loading channel catalog..."
                : channelsError ?? "The server did not return any supported channel type."}
            </p>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-6">
            <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
              <div className="min-w-0">
                <h2 className="text-base font-semibold">
                  {selectedDefinition.label}
                </h2>
              </div>

              {configuredChannel ? (
                <Button
                  variant="ghost"
                  size="sm"
                  className="shrink-0 self-start text-destructive hover:bg-destructive/10 hover:text-destructive"
                  onClick={() => void handleDelete()}
                >
                  <Trash2 className="mr-1.5 size-3.5" />
                  Delete
                </Button>
              ) : null}
            </div>

            <Separator className="opacity-40" />

            {matchingChannels.length > 1 ? (
              <section className="rounded-xl bg-amber-500/10 px-4 py-3 text-[12px] leading-5 text-amber-950 dark:text-amber-100">
                Multiple saved profiles were found for this transport. This
                panel is currently editing the first configured profile:
                <span className="ml-1 font-medium">{configuredChannel?.id}</span>
              </section>
            ) : null}

            <section className="space-y-4">
              {selectedProperties.length > 0 ? (
                <div className="divide-y divide-border/30">
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
                          className="flex items-center justify-between gap-3 py-3 text-[12px] text-foreground"
                        >
                          <div className="pr-4">
                            <p className="font-medium">{label}</p>
                            {description ? (
                              <p className="mt-1 text-[11px] text-muted-foreground">
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
                      <div key={key} className="py-3">
                        <label className="mb-1.5 block text-[12px] text-muted-foreground">
                          {label}
                          {selectedRequired.has(key) ? " *" : ""}
                          {configuredChannel && kind === "secret"
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
                          className="h-9 text-[13px]"
                        />
                        {description ? (
                          <p className="mt-1.5 text-[11px] leading-5 text-muted-foreground">
                            {description}
                          </p>
                        ) : null}
                      </div>
                    )
                  })}
                </div>
              ) : (
                <p className="text-[12px] text-muted-foreground">
                  This channel type does not expose configurable fields.
                </p>
              )}
            </section>

            <Separator className="opacity-40" />

            <label className="flex items-center justify-between gap-3 py-1 text-[12px] text-foreground">
                <div className="min-w-0">
                  <p className="font-medium">Enabled</p>
                  <p className="mt-1 text-[11px] text-muted-foreground">
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

            <Separator className="opacity-40" />

            <div className="flex flex-col-reverse gap-3 sm:flex-row sm:items-center sm:justify-between">
              <p className="text-[11px] leading-5 text-muted-foreground">
                {configuredChannel
                  ? "Changes are saved back to the current transport profile."
                  : "Saving creates a default profile for this transport type."}
              </p>
              <Button
                type="submit"
                disabled={submitting || !canSubmit}
                className="sm:min-w-[200px]"
              >
                {configuredChannel ? "Save Configuration" : "Create Configuration"}
              </Button>
            </div>
          </form>
        )}
      </div>
    </ScrollArea>
  )
}
