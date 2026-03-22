import { useEffect, useId, useMemo, useState, type FormEvent } from "react"
import { ArrowLeft, Plus, Search, Trash2, X } from "lucide-react"

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
import type {
  ChannelListItem,
  SupportedChannelDefinition,
  ModelConfig,
} from "@/lib/types"
import { cn } from "@/lib/utils"
import { useChannelsStore } from "@/stores/channels-store"
import { NEW_PROVIDER_SETTINGS_KEY, useChatStore } from "@/stores/chat-store"

type ModelFormRow = {
  id: string
  display_name: string
  limit_context: string
  limit_output: string
  supports_reasoning: boolean
}

function emptyModelRow(): ModelFormRow {
  return {
    id: "",
    display_name: "",
    limit_context: "",
    limit_output: "",
    supports_reasoning: false,
  }
}

function providerHost(baseUrl: string): string {
  try {
    const normalized =
      baseUrl.startsWith("http://") || baseUrl.startsWith("https://")
        ? baseUrl
        : `https://${baseUrl}`
    return new URL(normalized).host
  } catch {
    return baseUrl.replace(/^https?:\/\//, "")
  }
}

function providerProtocolLabel(kind: string): string {
  if (kind === "openai-responses") return "Responses"
  if (kind === "openai-chat-completions") return "Chat Completions"
  return kind
}

function channelConfigFields(definition: SupportedChannelDefinition): number {
  const schema = definition.config_schema as Record<string, unknown>
  const properties = schema.properties
  if (!properties || typeof properties !== "object") return 0
  return Object.keys(properties).length
}

function configuredChannelForTransport(
  configuredChannels: ChannelListItem[],
  transport: string
): ChannelListItem | null {
  return (
    configuredChannels.find((channel) => channel.transport === transport) ??
    null
  )
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
  const configuredChannels = useChannelsStore((s) => s.configuredChannels)
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
  const settingsScopeId = useId()

  const searchInputId = `${settingsScopeId}-search`
  const providerNameInputId = `${settingsScopeId}-provider-name`
  const providerProtocolInputId = `${settingsScopeId}-provider-protocol`
  const providerProtocolLabelId = `${providerProtocolInputId}-label`
  const providerApiKeyInputId = `${settingsScopeId}-provider-api-key`
  const providerApiKeyHintId = `${providerApiKeyInputId}-hint`
  const providerBaseUrlInputId = `${settingsScopeId}-provider-base-url`

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
      }))
  }

  const hasValidModel = models.some((model) => model.id.trim())

  function handleKindChange(value: string | null) {
    if (value) setKind(value)
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

  const filteredProviders = useMemo(
    () =>
      normalizedItemQuery
        ? providerList.filter((providerItem) => {
            return (
              providerItem.name.toLowerCase().includes(normalizedItemQuery) ||
              providerItem.kind.toLowerCase().includes(normalizedItemQuery)
            )
          })
        : providerList,
    [normalizedItemQuery, providerList]
  )

  const filteredChannels = useMemo(
    () =>
      normalizedItemQuery
        ? supportedChannels.filter((channel) => {
            return (
              channel.label.toLowerCase().includes(normalizedItemQuery) ||
              channel.transport.toLowerCase().includes(normalizedItemQuery)
            )
          })
        : supportedChannels,
    [normalizedItemQuery, supportedChannels]
  )

  const workspaceDescription = isProvidersSection
    ? "Provider connections and model catalogs"
    : "Channel transport profiles"

  const providerSubmitDisabled =
    submitting ||
    (!selectedProvider && !name.trim()) ||
    !hasValidModel ||
    (!selectedProvider && !apiKey.trim())

  const modelRowsWithId = models.filter((model) => model.id.trim())

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex items-start justify-between gap-3 border-b border-border/30 px-4 py-2.5">
        <div className="flex min-w-0 items-start gap-3">
          <button
            type="button"
            onClick={() => setView("chat")}
            className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
            aria-label="Back to chat"
          >
            <ArrowLeft className="size-3" />
          </button>
          <div className="min-w-0">
            <h1 className="mt-0.5 text-sm font-semibold tracking-tight">
              Settings Workbench
            </h1>
            <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
              {workspaceDescription}
            </p>
          </div>
        </div>

        <div className="flex shrink-0 flex-col gap-2 sm:flex-row sm:items-center">
          <div className="relative min-w-0 sm:w-[260px]">
            <label htmlFor={searchInputId} className="sr-only">
              {isProvidersSection ? "Search providers" : "Search channels"}
            </label>
            <Search className="pointer-events-none absolute top-1/2 left-3 size-3 -translate-y-1/2 text-muted-foreground/70" />
            <Input
              id={searchInputId}
              value={itemQuery}
              onChange={(event) => setItemQuery(event.target.value)}
              placeholder={
                isProvidersSection
                  ? "Filter providers by name or protocol"
                  : "Filter channels by label or transport"
              }
              className="h-9 pl-9 text-[13px]"
            />
          </div>

          {isProvidersSection ? (
            <Button
              type="button"
              size="sm"
              onClick={() => selectProviderName(NEW_PROVIDER_SETTINGS_KEY)}
              className="h-9 px-3"
            >
              <Plus className="size-3.5" />
              New Provider
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
                  <div>
                    <p className="text-[11px] font-medium tracking-[0.12em] text-foreground uppercase">
                      {isProvidersSection ? "Registry" : "Channel Catalog"}
                    </p>
                    <p className="mt-0.5 text-[11px] text-muted-foreground">
                      {isProvidersSection
                        ? "按可用性与连接信息快速判断目标 Provider"
                        : "按接入状态与字段规模判断下一步配置成本"}
                    </p>
                  </div>
                  <span className="font-mono text-[11px] text-muted-foreground tabular-nums">
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
                          ? "No providers yet. Start with New Provider in the top-right corner."
                          : "No matches found. Try filtering by name or protocol."}
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
                              "flex w-full flex-col gap-1 rounded-lg border px-3 py-2.5 text-left transition-colors",
                              isActive
                                ? "border-border/55 bg-accent/45 text-foreground"
                                : "border-transparent text-muted-foreground hover:border-border/30 hover:bg-accent/20 hover:text-foreground"
                            )}
                            aria-pressed={isActive}
                          >
                            <span className="flex items-start justify-between gap-2">
                              <span className="min-w-0">
                                <span className="block truncate text-[12px] font-medium">
                                  {providerItem.name}
                                </span>
                                <span className="mt-0.5 block truncate text-[11px] text-muted-foreground/90">
                                  {providerHost(providerItem.base_url)}
                                </span>
                              </span>
                              <span
                                className={cn(
                                  "mt-0.5 rounded-sm border px-1.5 py-0.5 text-[10px] font-medium",
                                  providerItem.active
                                    ? "border-[var(--trace-accent-strong)]/30 bg-[var(--trace-accent-strong)]/10 text-[var(--trace-accent-strong)]"
                                    : "border-border/30 text-muted-foreground"
                                )}
                              >
                                {providerItem.active ? "in use" : "standby"}
                              </span>
                            </span>
                            <span className="flex flex-wrap items-center gap-1.5 text-[10px] text-muted-foreground/85">
                              <span className="rounded-sm border border-border/30 px-1.5 py-0.5">
                                {providerProtocolLabel(providerItem.kind)}
                              </span>
                              <span className="rounded-sm border border-border/30 px-1.5 py-0.5 tabular-nums">
                                {providerItem.models.length} model
                                {providerItem.models.length === 1 ? "" : "s"}
                              </span>
                            </span>
                          </button>
                        )
                      })
                    )
                  ) : channelsLoading && supportedChannels.length === 0 ? (
                    <p className="px-3 py-4 text-[12px] text-muted-foreground">
                      Loading channel transports...
                    </p>
                  ) : filteredChannels.length === 0 ? (
                    <p className="px-3 py-4 text-[12px] text-muted-foreground">
                      {supportedChannels.length === 0 && !normalizedItemQuery
                        ? "The server has not returned any configurable channels yet."
                        : "No matches found. Try filtering by transport or label."}
                    </p>
                  ) : (
                    filteredChannels.map((channel) => {
                      const isActive = channel.transport === selectedTransport
                      const configured = configuredChannelForTransport(
                        configuredChannels,
                        channel.transport
                      )
                      const fieldCount = channelConfigFields(channel)

                      return (
                        <button
                          key={channel.transport}
                          type="button"
                          onClick={() => selectTransport(channel.transport)}
                          className={cn(
                            "flex w-full flex-col gap-1 rounded-lg border px-3 py-2.5 text-left transition-colors",
                            isActive
                              ? "border-border/55 bg-accent/45 text-foreground"
                              : "border-transparent text-muted-foreground hover:border-border/30 hover:bg-accent/20 hover:text-foreground"
                          )}
                          aria-pressed={isActive}
                        >
                          <span className="flex items-start justify-between gap-2">
                            <span className="min-w-0">
                              <span className="block truncate text-[12px] font-medium">
                                {channel.label}
                              </span>
                              <span className="mt-0.5 block truncate text-[11px] text-muted-foreground/90">
                                {channel.transport}
                              </span>
                            </span>
                            <span
                              className={cn(
                                "mt-0.5 rounded-sm border px-1.5 py-0.5 text-[10px] font-medium",
                                configured?.enabled
                                  ? "border-[var(--trace-accent-strong)]/30 bg-[var(--trace-accent-strong)]/10 text-[var(--trace-accent-strong)]"
                                  : configured
                                    ? "border-amber-500/40 bg-amber-500/10 text-amber-600"
                                    : "border-border/30 text-muted-foreground"
                              )}
                            >
                              {configured
                                ? configured.enabled
                                  ? "running"
                                  : "paused"
                                : "setup"}
                            </span>
                          </span>
                          <span className="flex flex-wrap items-center gap-1.5 text-[10px] text-muted-foreground/85">
                            <span className="rounded-sm border border-border/30 px-1.5 py-0.5 tabular-nums">
                              {fieldCount} field{fieldCount === 1 ? "" : "s"}
                            </span>
                            <span className="rounded-sm border border-border/30 px-1.5 py-0.5">
                              {configured ? configured.id : "no saved profile"}
                            </span>
                          </span>
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
                  <div className="shrink-0 border-b border-border/25 px-3 py-2.5">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0">
                        <div className="flex flex-wrap items-center gap-2">
                          <h2 className="truncate text-[15px] font-semibold">
                            {selectedProvider
                              ? selectedProvider.name
                              : "新建 Provider"}
                          </h2>
                          <Badge variant="outline" className="text-[10px]">
                            {selectedProvider
                              ? selectedProvider.active
                                ? "in use"
                                : "standby"
                              : "draft"}
                          </Badge>
                          <Badge variant="outline" className="text-[10px]">
                            {selectedProvider
                              ? providerProtocolLabel(selectedProvider.kind)
                              : providerProtocolLabel(kind)}
                          </Badge>
                        </div>
                        <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
                          {selectedProvider
                            ? `Host ${providerHost(selectedProvider.base_url)} · ${selectedProvider.models.length} models registered.`
                            : "Submit the connection settings to register this provider and make it available to sessions immediately."}
                        </p>
                      </div>

                      {selectedProvider ? (
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => void handleDeleteProvider()}
                          className="h-9 shrink-0 px-3 text-destructive hover:bg-destructive/10 hover:text-destructive"
                        >
                          <Trash2 className="size-3.5" />
                          Delete
                        </Button>
                      ) : null}
                    </div>
                  </div>

                  <form
                    onSubmit={handleSubmit}
                    className="flex min-h-0 flex-1 flex-col overflow-hidden"
                  >
                    <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-3">
                      <section className="rounded-xl border border-border/30 bg-card/70 p-3">
                        <div className="mb-2.5">
                          <p className="text-[11px] font-medium tracking-[0.12em] text-foreground uppercase">
                            Connection
                          </p>
                          <p className="mt-1 text-[11px] text-muted-foreground">
                            Name is the registry key and must be unique.
                            Protocol controls request mapping.
                          </p>
                        </div>

                        <div className="grid gap-2 sm:grid-cols-2">
                          <div className="space-y-1.5">
                            <label
                              htmlFor={providerNameInputId}
                              className="workspace-form-label"
                            >
                              Name
                            </label>
                            <Input
                              id={providerNameInputId}
                              value={name}
                              onChange={(event) => setName(event.target.value)}
                              placeholder="e.g. openai-main"
                              className="h-9 text-[13px]"
                              disabled={selectedProvider != null}
                            />
                          </div>

                          <div className="space-y-1.5">
                            <label
                              id={providerProtocolLabelId}
                              htmlFor={providerProtocolInputId}
                              className="workspace-form-label"
                            >
                              Protocol
                            </label>
                            <Select
                              value={kind}
                              onValueChange={handleKindChange}
                            >
                              <SelectTrigger
                                id={providerProtocolInputId}
                                aria-labelledby={providerProtocolLabelId}
                                className="h-9 w-full text-[13px]"
                              >
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
                      </section>

                      <section className="rounded-xl border border-border/30 bg-card/70 p-3">
                        <div className="mb-2.5">
                          <p className="text-[11px] font-medium tracking-[0.12em] text-foreground uppercase">
                            Authentication
                          </p>
                          <p className="mt-1 text-[11px] text-muted-foreground">
                            Required when creating a provider. Leave it blank
                            while editing to keep the current key.
                          </p>
                        </div>

                        <div className="grid gap-2 sm:grid-cols-2">
                          <div className="space-y-1.5">
                            <label
                              htmlFor={providerBaseUrlInputId}
                              className="workspace-form-label"
                            >
                              Base URL
                            </label>
                            <Input
                              id={providerBaseUrlInputId}
                              value={baseUrl}
                              onChange={(event) =>
                                setBaseUrl(event.target.value)
                              }
                              className="h-9 text-[13px]"
                            />
                            <p className="workspace-form-note">
                              This URL defines the request host and path prefix,
                              such as an OpenAI-compatible gateway.
                            </p>
                          </div>

                          <div className="space-y-1.5">
                            <label
                              htmlFor={providerApiKeyInputId}
                              className="workspace-form-label"
                            >
                              API key
                            </label>
                            {selectedProvider ? (
                              <p
                                id={providerApiKeyHintId}
                                className="workspace-form-note"
                              >
                                Leave blank to keep the current key.
                              </p>
                            ) : null}
                            <Input
                              id={providerApiKeyInputId}
                              type="text"
                              value={apiKey}
                              onChange={(event) =>
                                setApiKey(event.target.value)
                              }
                              placeholder="sk-..."
                              aria-describedby={
                                selectedProvider
                                  ? providerApiKeyHintId
                                  : undefined
                              }
                              className="h-9 text-[13px]"
                            />
                          </div>
                        </div>
                      </section>

                      <section className="rounded-xl border border-border/30 bg-card/70 p-3">
                        <div className="mb-2.5 flex flex-wrap items-start justify-between gap-2">
                          <div>
                            <p className="text-[11px] font-medium tracking-[0.12em] text-foreground uppercase">
                              Model Catalog
                            </p>
                            <p className="mt-1 text-[11px] text-muted-foreground">
                              At least one valid Model ID is required. Context
                              and output limits fall back to backend defaults
                              when left blank.
                            </p>
                          </div>

                          <div className="flex items-center gap-2">
                            <span className="rounded-sm border border-border/30 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground tabular-nums">
                              {modelRowsWithId.length} active
                            </span>
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              onClick={() =>
                                setModels((prev) => [emptyModelRow(), ...prev])
                              }
                              className="h-8 px-2.5"
                            >
                              <Plus className="size-3.5" />
                              Add model
                            </Button>
                          </div>
                        </div>

                        <div className="overflow-x-auto rounded-lg border border-border/25">
                          <div className="min-w-[840px]">
                            <div className="grid grid-cols-[minmax(220px,2fr)_minmax(170px,1.4fr)_110px_110px_120px_44px] gap-2 border-b border-border/20 bg-muted/[0.12] px-2.5 py-2 text-[10px] font-medium tracking-[0.08em] text-muted-foreground uppercase">
                              <span>Model ID</span>
                              <span>Display Name</span>
                              <span>Context</span>
                              <span>Output</span>
                              <span>Reasoning</span>
                              <span className="text-center">-</span>
                            </div>

                            <div className="divide-y divide-border/20">
                              {models.map((row, index) => (
                                <div
                                  key={`${row.id}:${index}`}
                                  className="grid grid-cols-[minmax(220px,2fr)_minmax(170px,1.4fr)_110px_110px_120px_44px] gap-2 px-2.5 py-2"
                                >
                                  <Input
                                    id={`${settingsScopeId}-model-id-${index}`}
                                    value={row.id}
                                    onChange={(event) =>
                                      updateModelRow(index, {
                                        id: event.target.value,
                                      })
                                    }
                                    placeholder="gpt-5.4"
                                    className="h-9 text-[12px]"
                                    aria-label={`Model ${index + 1} ID`}
                                  />

                                  <Input
                                    id={`${settingsScopeId}-model-display-name-${index}`}
                                    value={row.display_name}
                                    onChange={(event) =>
                                      updateModelRow(index, {
                                        display_name: event.target.value,
                                      })
                                    }
                                    placeholder="Optional display name"
                                    className="h-9 text-[12px]"
                                    aria-label={`Model ${index + 1} display name`}
                                  />

                                  <Input
                                    id={`${settingsScopeId}-model-context-limit-${index}`}
                                    value={row.limit_context}
                                    onChange={(event) =>
                                      updateModelRow(index, {
                                        limit_context: event.target.value,
                                      })
                                    }
                                    placeholder="ctx"
                                    className="h-9 text-[12px]"
                                    inputMode="numeric"
                                    aria-label={`Model ${index + 1} context limit`}
                                  />

                                  <Input
                                    id={`${settingsScopeId}-model-output-limit-${index}`}
                                    value={row.limit_output}
                                    onChange={(event) =>
                                      updateModelRow(index, {
                                        limit_output: event.target.value,
                                      })
                                    }
                                    placeholder="out"
                                    className="h-9 text-[12px]"
                                    inputMode="numeric"
                                    aria-label={`Model ${index + 1} output limit`}
                                  />

                                  <div className="flex items-center justify-center rounded-md border border-border/25 bg-background/60 px-2">
                                    <Switch
                                      checked={row.supports_reasoning}
                                      onCheckedChange={(checked: boolean) =>
                                        updateModelRow(index, {
                                          supports_reasoning: checked,
                                        })
                                      }
                                      size="default"
                                      aria-label={`Model ${index + 1} reasoning support`}
                                    />
                                  </div>

                                  <div className="flex items-center justify-center">
                                    {models.length > 1 ? (
                                      <Button
                                        type="button"
                                        variant="ghost"
                                        size="icon-sm"
                                        onClick={() => removeModelRow(index)}
                                        aria-label={`Remove model ${index + 1}`}
                                        className="size-8 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                                      >
                                        <X className="size-3.5" />
                                      </Button>
                                    ) : null}
                                  </div>
                                </div>
                              ))}
                            </div>
                          </div>
                        </div>

                        <p className="mt-2 text-[11px] text-muted-foreground">
                          The Reasoning switch indicates whether this model
                          supports session-level thinking controls.
                        </p>
                      </section>
                    </div>

                    <div className="shrink-0 border-t border-border/20 px-3 py-2.5">
                      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                        <p className="text-[11px] text-muted-foreground">
                          {!hasValidModel
                            ? "Enter at least one Model ID before saving."
                            : selectedProvider
                              ? "Submitting will update the current provider."
                              : "Submitting will create a new provider and select it automatically."}
                        </p>
                        <Button
                          type="submit"
                          disabled={providerSubmitDisabled}
                          className="min-h-9 min-w-[190px]"
                        >
                          <Plus className="size-3.5" />
                          {selectedProvider
                            ? "Save provider"
                            : "Create provider"}
                        </Button>
                      </div>
                    </div>
                  </form>
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
