import { useEffect, useId, useMemo, useState, type FormEvent } from "react"
import { ArrowLeft, Plus, Search, Trash2 } from "lucide-react"

import { ChannelsPanel } from "@/features/channels"
import {
  ProviderAuthenticationSection,
  ProviderConnectionSection,
  ProviderModelCatalogSection,
  type ModelFormRow,
} from "./provider-form-sections"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type {
  ChannelListItem,
  SupportedChannelDefinition,
  ModelConfig,
} from "@/lib/types"
import { cn } from "@/lib/utils"
import { useChannelsStore } from "@/stores/channels-store"
import { NEW_PROVIDER_SETTINGS_KEY } from "@/stores/chat-store"
import { useProviderRegistryStore } from "@/stores/provider-registry-store"
import { useWorkbenchStore } from "@/stores/workbench-store"

const SETTINGS_META_LABEL_FOREGROUND = "workspace-section-label text-foreground"
const SETTINGS_PANEL_HELP_TEXT = "workspace-panel-copy"
const SETTINGS_BADGE =
  "text-ui-xs rounded-sm border border-border/30 px-1.5 py-0.5 font-medium"
const SETTINGS_INFO_TEXT = "workspace-meta"
const SETTINGS_MONO_COUNT = "workspace-code text-muted-foreground"

function nextModelRowKey(): string {
  return `model-row-${Math.random().toString(36).slice(2, 10)}`
}

function emptyModelRow(): ModelFormRow {
  return {
    _key: nextModelRowKey(),
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
  if (kind === "openai-responses") return "Responses API"
  if (kind === "openai-chat-completions") return "Chat Completions API"
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
  const providerList = useProviderRegistryStore((s) => s.providerList)
  const storeCreateProvider = useProviderRegistryStore((s) => s.createProvider)
  const storeUpdateProvider = useProviderRegistryStore((s) => s.updateProvider)
  const storeDeleteProvider = useProviderRegistryStore((s) => s.deleteProvider)
  const setView = useWorkbenchStore((s) => s.setView)
  const settingsSection = useWorkbenchStore((s) => s.settingsSection)
  const selectedProviderName = useWorkbenchStore((s) => s.selectedProviderName)
  const selectProviderName = useWorkbenchStore((s) => s.selectProviderName)
  const reconcileProviderSelection = useWorkbenchStore(
    (s) => s.reconcileProviderSelection
  )

  const supportedChannels = useChannelsStore((s) => s.supportedChannels)
  const selectedTransport = useChannelsStore((s) => s.selectedTransport)
  const channelsLoading = useChannelsStore((s) => s.loading)
  const configuredChannels = useChannelsStore((s) => s.configuredChannels)
  const selectTransport = useChannelsStore((s) => s.selectTransport)

  const selectedProvider =
    providerList.find((provider) => provider.id === selectedProviderName) ??
    null

  const [label, setLabel] = useState("")
  const [adapter, setAdapter] = useState("openai-responses")
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
      setLabel("")
      setAdapter("openai-responses")
      setApiKey("")
      setBaseUrl("https://api.openai.com/v1")
      setModels([emptyModelRow()])
      return
    }

    setLabel(selectedProvider.label)
    setAdapter(selectedProvider.adapter)
    setApiKey("")
    setBaseUrl(selectedProvider.base_url)
    setModels(
      selectedProvider.models.map((model) => ({
        _key: nextModelRowKey(),
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

  useEffect(() => {
    reconcileProviderSelection(providerList)
  }, [providerList, reconcileProviderSelection])

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

  function handleAdapterChange(value: string | null) {
    if (value) setAdapter(value)
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!hasValidModel) return

    setSubmitting(true)

    try {
      const builtModels = buildModels()

      if (selectedProvider) {
        const body: Record<string, unknown> = {
          label: label.trim(),
          adapter,
          models: builtModels,
          base_url: baseUrl.trim(),
        }

        if (apiKey.trim()) {
          body.credential = {
            type: "api_key",
            value: apiKey.trim(),
          }
        }

        await storeUpdateProvider(
          selectedProvider.id,
          body as Parameters<typeof storeUpdateProvider>[1]
        )
        return
      }

      const providerId = label.trim()
      await storeCreateProvider({
        id: providerId,
        label: label.trim(),
        adapter,
        credential: {
          type: "api_key",
          value: apiKey.trim(),
        },
        models: builtModels,
        base_url: baseUrl.trim(),
      })
      selectProviderName(providerId)
    } finally {
      setSubmitting(false)
    }
  }

  async function handleDeleteProvider() {
    if (!selectedProvider) return

    const deletingLastProvider = providerList.length <= 1
    await storeDeleteProvider(selectedProvider.id)

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
              providerItem.id.toLowerCase().includes(normalizedItemQuery) ||
              providerItem.label.toLowerCase().includes(normalizedItemQuery) ||
              providerItem.adapter.toLowerCase().includes(normalizedItemQuery)
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
    ? "Manage provider connections and model catalogs"
    : "Manage channel transport profiles"

  const providerSubmitDisabled =
    submitting ||
    (!selectedProvider && !label.trim()) ||
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
            className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
            aria-label="Back to chat"
          >
            <ArrowLeft className="size-3" />
          </button>
          <div className="min-w-0">
            <h1 className="text-ui-xs mt-0.5 font-semibold tracking-tight text-foreground">
              Settings
            </h1>
            <p className="workspace-panel-copy mt-1 text-muted-foreground">
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
              className="h-8 pl-9"
            />
          </div>

          {isProvidersSection ? (
            <Button
              type="button"
              size="sm"
              onClick={() => selectProviderName(NEW_PROVIDER_SETTINGS_KEY)}
              className="text-ui-xs h-8 bg-foreground px-3 tracking-[0.04em] text-background normal-case hover:bg-foreground/92"
            >
              <Plus className="size-3.5" />
              New provider
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
                    <p className={SETTINGS_META_LABEL_FOREGROUND}>
                      {isProvidersSection ? "Providers" : "Channel catalog"}
                    </p>
                    <p className={`mt-0.5 ${SETTINGS_PANEL_HELP_TEXT}`}>
                      {isProvidersSection
                        ? "Review hosts, protocols, and models at a glance."
                        : "Compare transport status and setup scope at a glance."}
                    </p>
                  </div>
                  <span className={SETTINGS_MONO_COUNT}>
                    {isProvidersSection
                      ? filteredProviders.length
                      : filteredChannels.length}
                  </span>
                </div>
              </div>

              <div className="min-h-0 flex-1 overflow-y-auto p-2.5">
                <div className="space-y-1.5">
                  {isProvidersSection ? (
                    filteredProviders.length === 0 ? (
                      <p className="workspace-panel-copy px-3 py-4 text-muted-foreground">
                        {providerList.length === 0 && !normalizedItemQuery
                          ? "No providers yet. Create one to get started."
                          : "No matches found. Try filtering by name or protocol."}
                      </p>
                    ) : (
                      filteredProviders.map((providerItem) => {
                        const isActive =
                          providerItem.id === selectedProviderName &&
                          selectedProviderName !== NEW_PROVIDER_SETTINGS_KEY

                        return (
                          <button
                            key={providerItem.id}
                            type="button"
                            onClick={() => selectProviderName(providerItem.id)}
                            className={cn(
                              "flex w-full flex-col gap-1.5 rounded-lg border px-3 py-3 text-left transition-colors",
                              isActive
                                ? "border-border/55 bg-muted/65 text-foreground"
                                : "border-transparent text-muted-foreground hover:border-border/30 hover:bg-muted/45 hover:text-foreground"
                            )}
                            aria-pressed={isActive}
                          >
                            <span className="flex items-start justify-between gap-2">
                              <span className="min-w-0">
                                <span className="text-ui-sm block truncate font-medium tracking-[0.01em] text-foreground">
                                  {providerItem.label}
                                </span>
                                <span
                                  className={`mt-0.5 block ${SETTINGS_INFO_TEXT}`}
                                >
                                  {providerHost(providerItem.base_url)}
                                </span>
                              </span>
                            </span>
                            <span className="text-ui-xs flex flex-wrap items-center gap-1.5 text-muted-foreground/85">
                              <span className="rounded-sm border border-border/30 px-1.5 py-0.5">
                                {providerProtocolLabel(providerItem.adapter)}
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
                    <p className="workspace-panel-copy px-3 py-4 text-muted-foreground">
                      Loading channel transports...
                    </p>
                  ) : filteredChannels.length === 0 ? (
                    <p className="workspace-panel-copy px-3 py-4 text-muted-foreground">
                      {supportedChannels.length === 0 && !normalizedItemQuery
                        ? "No configurable channels are available yet."
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
                            "flex w-full flex-col gap-1.5 rounded-lg border px-3 py-3 text-left transition-colors",
                            isActive
                              ? "border-border/55 bg-muted/65 text-foreground"
                              : "border-transparent text-muted-foreground hover:border-border/30 hover:bg-muted/45 hover:text-foreground"
                          )}
                          aria-pressed={isActive}
                        >
                          <span className="flex items-start justify-between gap-2">
                            <span className="min-w-0">
                              <span className="text-ui-sm block truncate font-medium tracking-[0.01em] text-foreground">
                                {channel.label}
                              </span>
                              <span className="workspace-meta mt-0.5 block truncate text-muted-foreground/90">
                                {channel.transport}
                              </span>
                            </span>
                            <span
                              className={cn(
                                SETTINGS_BADGE,
                                configured?.enabled
                                  ? "border-border/40 bg-muted/55 text-foreground/80"
                                  : configured
                                    ? "border-amber-500/40 bg-amber-500/10 text-amber-600"
                                    : "text-muted-foreground"
                              )}
                            >
                              {configured
                                ? configured.enabled
                                  ? "running"
                                  : "paused"
                                : "setup"}
                            </span>
                          </span>
                          <span className="text-ui-xs flex flex-wrap items-center gap-1.5 text-muted-foreground/85">
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
                          <h2 className="text-ui-xs truncate font-semibold text-foreground">
                            {selectedProvider
                              ? selectedProvider.label
                              : "New provider"}
                          </h2>
                          <Badge variant="outline" className="text-ui-xs">
                            {selectedProvider
                              ? providerProtocolLabel(selectedProvider.adapter)
                              : providerProtocolLabel(adapter)}
                          </Badge>
                        </div>
                        <p className="workspace-panel-copy mt-1 text-muted-foreground">
                          {selectedProvider
                            ? `Host ${providerHost(selectedProvider.base_url)} · ${selectedProvider.models.length} models registered.`
                            : "Save these settings to add a provider and make it available to new sessions."}
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
                          Delete provider
                        </Button>
                      ) : null}
                    </div>
                  </div>

                  <form
                    onSubmit={handleSubmit}
                    className="flex min-h-0 flex-1 flex-col overflow-hidden"
                  >
                    <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-3">
                      <ProviderConnectionSection
                        name={label}
                        kind={adapter}
                        providerNameInputId={providerNameInputId}
                        providerProtocolInputId={providerProtocolInputId}
                        providerProtocolLabelId={providerProtocolLabelId}
                        selectedProviderLocked={selectedProvider != null}
                        onNameChange={setLabel}
                        onKindChange={handleAdapterChange}
                      />

                      <ProviderAuthenticationSection
                        selectedProvider={selectedProvider != null}
                        providerBaseUrlInputId={providerBaseUrlInputId}
                        providerApiKeyInputId={providerApiKeyInputId}
                        providerApiKeyHintId={providerApiKeyHintId}
                        baseUrl={baseUrl}
                        apiKey={apiKey}
                        onBaseUrlChange={setBaseUrl}
                        onApiKeyChange={setApiKey}
                      />

                      <ProviderModelCatalogSection
                        modelRowsWithId={modelRowsWithId.length}
                        models={models}
                        settingsScopeId={settingsScopeId}
                        onAddModel={() =>
                          setModels((prev) => [emptyModelRow(), ...prev])
                        }
                        onUpdateModelRow={updateModelRow}
                        onRemoveModelRow={removeModelRow}
                      />
                    </div>

                    <div className="shrink-0 border-t border-border/20 px-3 py-2.5">
                      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                        <p className="workspace-meta text-muted-foreground">
                          {!hasValidModel
                            ? "Add at least one model ID before saving."
                            : selectedProvider
                              ? "Saving updates this provider."
                              : "Saving creates this provider and selects it automatically."}
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
