import { useEffect, useState, type FormEvent } from "react"
import {
  ArrowLeft,
  Plus,
  Search,
  Settings2,
  Trash2,
  Waypoints,
  X,
} from "lucide-react"

import { ChannelsPanel } from "@/components/channels-panel"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import type { ModelConfig } from "@/lib/types"
import { cn } from "@/lib/utils"
import { useChannelsStore } from "@/stores/channels-store"
import { NEW_PROVIDER_SETTINGS_KEY, useChatStore } from "@/stores/chat-store"

type ModelFormRow = {
  id: string
  display_name: string
  limit_context: string
  limit_output: string
  supports_reasoning: boolean
  reasoning_effort: string
}

function emptyModelRow(): ModelFormRow {
  return {
    id: "",
    display_name: "",
    limit_context: "",
    limit_output: "",
    supports_reasoning: false,
    reasoning_effort: "medium",
  }
}

export function SettingsPanel() {
  const providerList = useChatStore((s) => s.providerList)
  const setView = useChatStore((s) => s.setView)
  const settingsSection = useChatStore((s) => s.settingsSection)
  const selectedProviderName = useChatStore((s) => s.selectedProviderName)
  const selectProviderName = useChatStore((s) => s.selectProviderName)
  const storeCreateProvider = useChatStore((s) => s.createProvider)
  const storeUpdateProvider = useChatStore((s) => s.updateProvider)
  const storeDeleteProvider = useChatStore((s) => s.deleteProvider)

  const supportedChannels = useChannelsStore((s) => s.supportedChannels)
  const selectedTransport = useChannelsStore((s) => s.selectedTransport)
  const channelsLoading = useChannelsStore((s) => s.loading)
  const selectTransport = useChannelsStore((s) => s.selectTransport)

  const selectedProvider =
    providerList.find((provider) => provider.name === selectedProviderName) ??
    null

  const [name, setName] = useState("")
  const [kind, setKind] = useState("openai-responses")
  const [apiKey, setApiKey] = useState("")
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1")
  const [models, setModels] = useState<ModelFormRow[]>([emptyModelRow()])
  const [itemQuery, setItemQuery] = useState("")
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    if (!selectedProvider) {
      setName("")
      setKind("openai-responses")
      setApiKey("")
      setBaseUrl("https://api.openai.com/v1")
      setModels([emptyModelRow()])
      return
    }

    setName(selectedProvider.name)
    setKind(selectedProvider.kind)
    setApiKey("")
    setBaseUrl(selectedProvider.base_url)
    setModels(
      selectedProvider.models.map((model) => ({
        id: model.id,
        display_name: model.display_name ?? "",
        limit_context: model.limit?.context?.toString() ?? "",
        limit_output: model.limit?.output?.toString() ?? "",
        supports_reasoning: model.supports_reasoning,
        reasoning_effort: model.reasoning_effort ?? "medium",
      }))
    )
  }, [selectedProvider])

  useEffect(() => {
    setItemQuery("")
  }, [settingsSection])

  function updateModelRow(index: number, patch: Partial<ModelFormRow>) {
    setModels((prev) =>
      prev.map((row, rowIndex) =>
        rowIndex === index ? { ...row, ...patch } : row
      )
    )
  }

  function removeModelRow(index: number) {
    setModels((prev) => prev.filter((_, rowIndex) => rowIndex !== index))
  }

  function buildModels(): ModelConfig[] {
    const parseLimitValue = (value: string) => {
      const trimmed = value.trim()
      if (!trimmed) return null
      const parsed = Number.parseInt(trimmed, 10)
      return Number.isFinite(parsed) ? parsed : null
    }

    return models
      .filter((model) => model.id.trim())
      .map((model) => ({
        id: model.id.trim(),
        display_name: model.display_name.trim() || null,
        limit: {
          context: parseLimitValue(model.limit_context),
          output: parseLimitValue(model.limit_output),
        },
        default_temperature: null,
        supports_reasoning: model.supports_reasoning,
        reasoning_effort: model.supports_reasoning
          ? model.reasoning_effort
          : null,
      }))
  }

  const hasValidModel = models.some((model) => model.id.trim())

  function handleKindChange(value: string | null) {
    if (value) setKind(value)
  }

  function handleReasoningEffortChange(index: number, value: string | null) {
    if (value) {
      updateModelRow(index, { reasoning_effort: value })
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!hasValidModel) return

    setSubmitting(true)

    try {
      const builtModels = buildModels()

      if (selectedProvider) {
        const body: Record<string, unknown> = {
          kind,
          models: builtModels,
          active_model: builtModels[0]?.id,
          base_url: baseUrl.trim(),
        }

        if (apiKey.trim()) body.api_key = apiKey.trim()

        await storeUpdateProvider(
          selectedProvider.name,
          body as Parameters<typeof storeUpdateProvider>[1]
        )
        return
      }

      const providerName = name.trim()
      await storeCreateProvider({
        name: providerName,
        kind,
        models: builtModels,
        active_model: builtModels[0]?.id,
        api_key: apiKey.trim(),
        base_url: baseUrl.trim(),
      })
      selectProviderName(providerName)
    } finally {
      setSubmitting(false)
    }
  }

  async function handleDeleteProvider() {
    if (!selectedProvider) return

    const deletingLastProvider = providerList.length <= 1
    await storeDeleteProvider(selectedProvider.name)

    if (deletingLastProvider) {
      selectProviderName(NEW_PROVIDER_SETTINGS_KEY)
    }
  }

  const isProvidersSection = settingsSection === "providers"
  const normalizedItemQuery = itemQuery.trim().toLowerCase()

  const filteredProviders = normalizedItemQuery
    ? providerList.filter((providerItem) => {
        return (
          providerItem.name.toLowerCase().includes(normalizedItemQuery) ||
          providerItem.kind.toLowerCase().includes(normalizedItemQuery)
        )
      })
    : providerList

  const filteredChannels = normalizedItemQuery
    ? supportedChannels.filter((channel) => {
        return (
          channel.label.toLowerCase().includes(normalizedItemQuery) ||
          channel.transport.toLowerCase().includes(normalizedItemQuery)
        )
      })
    : supportedChannels

  const settingsDescription = isProvidersSection
    ? "Manage provider registry entries, endpoints, and model catalogs from the shared workspace."
    : "Manage channel transports and runtime profiles from the shared workspace."

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex items-center justify-between gap-2 border-b border-border/30 px-4 py-2.5">
        <div className="flex items-start gap-3">
          <button
            type="button"
            onClick={() => setView("chat")}
            className="mt-0.5 flex size-7 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
          >
            <ArrowLeft className="size-3.5" />
          </button>
          <div>
            <div className="flex flex-wrap items-center gap-2">
              <h1 className="text-sm font-semibold tracking-tight">
                Settings
              </h1>
              <Badge variant="secondary" className="text-[10px]">
                {isProvidersSection ? "providers" : "channels"}
              </Badge>
            </div>
            <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
              {settingsDescription}
            </p>
          </div>
        </div>

        <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
          <div className="relative min-w-0 sm:w-[260px]">
            <Search className="pointer-events-none absolute top-1/2 left-3 size-3 -translate-y-1/2 text-muted-foreground/70" />
            <Input
              value={itemQuery}
              onChange={(event) => setItemQuery(event.target.value)}
              placeholder={
                isProvidersSection
                  ? "Search providers..."
                  : "Search channels..."
              }
              className="h-8 pl-8 text-[12px]"
            />
          </div>

          {isProvidersSection ? (
            <Button
              type="button"
              size="sm"
              onClick={() => selectProviderName(NEW_PROVIDER_SETTINGS_KEY)}
            >
              <Plus className="size-3.5" />
              Add Provider
            </Button>
          ) : null}
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-4 py-3">
        <div className="mx-auto flex min-h-0 w-full max-w-[1440px] flex-1 flex-col gap-2">
          <div className="grid min-h-0 flex-1 overflow-hidden rounded-xl border border-border/30 bg-card/70 shadow-[var(--workspace-shadow)] xl:grid-cols-[260px_minmax(0,1fr)]">
            <div className="flex min-h-0 flex-col overflow-hidden">
              <div className="shrink-0 border-b border-border/25 px-3 py-2.5">
                <div className="flex items-center justify-between gap-2">
                  <p className="text-[12px] font-medium tracking-[0.08em] text-foreground uppercase">
                    {isProvidersSection ? "Provider List" : "Channel List"}
                  </p>
                  <span className="font-mono text-[11px] text-muted-foreground">
                    {isProvidersSection
                      ? filteredProviders.length
                      : filteredChannels.length}
                  </span>
                </div>
              </div>

              <div className="min-h-0 flex-1 overflow-y-auto p-2.5">
                <div className="space-y-1">
                  {isProvidersSection ? (
                    filteredProviders.length === 0 ? (
                      <p className="px-3 py-4 text-[12px] text-muted-foreground">
                        {providerList.length === 0 && !normalizedItemQuery
                          ? "No providers configured yet."
                          : "No matching providers."}
                      </p>
                    ) : (
                      filteredProviders.map((providerItem) => {
                        const isActive =
                          providerItem.name === selectedProviderName &&
                          selectedProviderName !== NEW_PROVIDER_SETTINGS_KEY

                        return (
                          <button
                            key={providerItem.name}
                            type="button"
                            onClick={() =>
                              selectProviderName(providerItem.name)
                            }
                            className={cn(
                              "flex w-full items-start gap-2 rounded-lg border px-2.5 py-2 text-left transition-colors",
                              isActive
                                ? "border-border/55 bg-accent/45 text-foreground"
                                : "border-transparent text-muted-foreground hover:border-border/30 hover:bg-accent/20 hover:text-foreground"
                            )}
                          >
                            <span className="min-w-0 flex-1">
                              <span className="block truncate text-[12px] font-medium">
                                {providerItem.name}
                              </span>
                              <span className="mt-0.5 block truncate text-[10px] text-muted-foreground/80">
                                {providerItem.kind}
                              </span>
                            </span>
                            <span
                              className={cn(
                                "mt-1 size-2 rounded-full",
                                providerItem.active
                                  ? "bg-blue-400"
                                  : "bg-muted-foreground/30"
                              )}
                            />
                          </button>
                        )
                      })
                    )
                  ) : channelsLoading && supportedChannels.length === 0 ? (
                    <p className="px-3 py-4 text-[12px] text-muted-foreground">
                      Loading channels...
                    </p>
                  ) : filteredChannels.length === 0 ? (
                    <p className="px-3 py-4 text-[12px] text-muted-foreground">
                      {supportedChannels.length === 0 && !normalizedItemQuery
                        ? "No supported channels available."
                        : "No matching channels."}
                    </p>
                  ) : (
                    filteredChannels.map((channel) => {
                      const isActive = channel.transport === selectedTransport

                      return (
                        <button
                          key={channel.transport}
                          type="button"
                          onClick={() => selectTransport(channel.transport)}
                          className={cn(
                            "flex w-full items-start gap-2 rounded-lg border px-2.5 py-2 text-left transition-colors",
                            isActive
                              ? "border-border/55 bg-accent/45 text-foreground"
                              : "border-transparent text-muted-foreground hover:border-border/30 hover:bg-accent/20 hover:text-foreground"
                          )}
                        >
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-[12px] font-medium">
                              {channel.label}
                            </span>
                            <span className="mt-0.5 block truncate text-[10px] text-muted-foreground/80">
                              {channel.transport}
                            </span>
                          </span>
                          <span
                            className={cn(
                              "mt-1 size-2 rounded-full",
                              isActive
                                ? "bg-blue-400"
                                : "bg-muted-foreground/30"
                            )}
                          />
                        </button>
                      )
                    })
                  )}
                </div>
              </div>
            </div>

            <div className="flex min-h-0 flex-col overflow-hidden border-l border-border/25">
              {isProvidersSection ? (
                <>
                  <div className="shrink-0 border-b border-border/25 px-3 py-2">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <h2 className="truncate text-[14px] font-semibold">
                            {selectedProvider
                              ? selectedProvider.name
                              : "New Provider"}
                          </h2>
                          <Badge variant="outline" className="text-[10px]">
                            {selectedProvider
                              ? selectedProvider.active
                                ? "Active"
                                : "Inactive"
                              : "Draft"}
                          </Badge>
                        </div>
                      </div>

                      {selectedProvider ? (
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => void handleDeleteProvider()}
                          className="shrink-0 text-destructive hover:bg-destructive/10 hover:text-destructive"
                        >
                          <Trash2 className="size-3.5" />
                          Delete
                        </Button>
                      ) : null}
                    </div>
                  </div>

                  <div className="min-h-0 flex-1 overflow-y-auto p-3">
                    <form onSubmit={handleSubmit} className="space-y-3">
                      <div className="grid gap-2 sm:grid-cols-2">
                        <div className="space-y-1.5">
                          <label className="workspace-form-label">Name</label>
                          <Input
                            value={name}
                            onChange={(event) => setName(event.target.value)}
                            placeholder="e.g. openai-main"
                            className="h-8 text-[12px]"
                            disabled={selectedProvider != null}
                          />
                        </div>

                        <div className="space-y-1.5">
                          <label className="workspace-form-label">
                            Protocol
                          </label>
                          <Select value={kind} onValueChange={handleKindChange}>
                            <SelectTrigger className="h-8 w-full text-[12px]">
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

                      <div className="space-y-1.5">
                        <label className="workspace-form-label">
                          API key
                          {selectedProvider
                            ? " (leave blank to keep existing)"
                            : ""}
                        </label>
                        <Input
                          type="text"
                          value={apiKey}
                          onChange={(event) => setApiKey(event.target.value)}
                          placeholder="sk-..."
                          className="h-8 text-[12px]"
                        />
                      </div>

                      <div className="space-y-1.5">
                        <label className="workspace-form-label">Base URL</label>
                        <Input
                          value={baseUrl}
                          onChange={(event) => setBaseUrl(event.target.value)}
                          className="h-8 text-[12px]"
                        />
                      </div>

                      <div className="space-y-2 border-t border-border/20 pt-3">
                        <div className="flex flex-wrap items-center justify-between gap-3">
                          <p className="workspace-form-label">Models</p>

                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() =>
                              setModels((prev) => [emptyModelRow(), ...prev])
                            }
                          >
                            <Plus className="size-3.5" />
                            Add Model
                          </Button>
                        </div>

                        <div className="space-y-2">
                          {models.map((row, index) => (
                            <div
                              key={`${row.id}:${index}`}
                              className="workspace-panel-soft px-2.5 py-2.5"
                            >
                              <div className="flex items-start justify-between gap-2">
                                <p className="text-sm font-medium text-foreground">
                                  Model {index + 1}
                                </p>

                                {models.length > 1 ? (
                                  <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-sm"
                                    onClick={() => removeModelRow(index)}
                                    aria-label={`Remove model ${index + 1}`}
                                    className="text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                                  >
                                    <X className="size-3.5" />
                                  </Button>
                                ) : null}
                              </div>

                              <div className="mt-2 grid gap-2 md:grid-cols-2">
                                <div className="space-y-1.5">
                                  <label className="workspace-form-label">
                                    Model ID
                                  </label>
                                  <Input
                                    value={row.id}
                                    onChange={(event) =>
                                      updateModelRow(index, {
                                        id: event.target.value,
                                      })
                                    }
                                    placeholder="e.g. gpt-5.4"
                                    className="h-8 text-[12px]"
                                  />
                                </div>

                                <div className="space-y-1.5">
                                  <label className="workspace-form-label">
                                    Display name
                                  </label>
                                  <Input
                                    value={row.display_name}
                                    onChange={(event) =>
                                      updateModelRow(index, {
                                        display_name: event.target.value,
                                      })
                                    }
                                    placeholder="Optional label shown in the UI"
                                    className="h-8 text-[12px]"
                                  />
                                </div>

                                <div className="space-y-1.5">
                                  <label className="workspace-form-label">
                                    Context limit
                                  </label>
                                  <Input
                                    value={row.limit_context}
                                    onChange={(event) =>
                                      updateModelRow(index, {
                                        limit_context: event.target.value,
                                      })
                                    }
                                    placeholder="Context limit"
                                    className="h-8 text-[12px]"
                                    inputMode="numeric"
                                  />
                                </div>

                                <div className="space-y-1.5">
                                  <label className="workspace-form-label">
                                    Output limit
                                  </label>
                                  <Input
                                    value={row.limit_output}
                                    onChange={(event) =>
                                      updateModelRow(index, {
                                        limit_output: event.target.value,
                                      })
                                    }
                                    placeholder="Output limit"
                                    className="h-8 text-[12px]"
                                    inputMode="numeric"
                                  />
                                </div>
                              </div>

                              <div className="mt-2 flex flex-col gap-2 rounded-xl border border-border/20 bg-background/55 px-2.5 py-2 sm:flex-row sm:items-center sm:justify-between">
                                <label className="flex items-center gap-2 text-sm text-foreground">
                                  <Switch
                                    checked={row.supports_reasoning}
                                    onCheckedChange={(checked: boolean) =>
                                      updateModelRow(index, {
                                        supports_reasoning: checked,
                                      })
                                    }
                                  />
                                  <span>
                                    Reasoning support
                                    <span className="mt-0.5 block text-[11px] text-muted-foreground">
                                      Only expose reasoning effort controls for
                                      models that explicitly support them.
                                    </span>
                                  </span>
                                </label>

                                {row.supports_reasoning ? (
                                  <Select
                                    value={row.reasoning_effort}
                                    onValueChange={(value) =>
                                      handleReasoningEffortChange(index, value)
                                    }
                                  >
                                    <SelectTrigger
                                      className="h-8 w-full text-[12px] sm:w-[160px]"
                                      size="sm"
                                    >
                                      <SelectValue />
                                    </SelectTrigger>
                                    <SelectContent>
                                      <SelectItem value="low">Low</SelectItem>
                                      <SelectItem value="medium">
                                        Medium
                                      </SelectItem>
                                      <SelectItem value="high">High</SelectItem>
                                    </SelectContent>
                                  </Select>
                                ) : null}
                              </div>
                            </div>
                          ))}
                        </div>
                      </div>

                      <div className="flex justify-end border-t border-border/20 pt-3">
                        <Button
                          type="submit"
                          disabled={
                            submitting ||
                            (!selectedProvider && !name.trim()) ||
                            !hasValidModel ||
                            (!selectedProvider && !apiKey.trim())
                          }
                          className="min-w-[190px]"
                        >
                          <Plus className="size-3.5" />
                          {selectedProvider
                            ? "Update Provider"
                            : "Create Provider"}
                        </Button>
                      </div>
                    </form>
                  </div>
                </>
              ) : (
                <div className="min-h-0 flex-1 overflow-y-auto p-3">
                  <ChannelsPanel embedded />
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
