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

const SETTINGS_BADGE =
  "text-ui-xs rounded-sm border border-border/30 px-1.5 py-0.5 font-medium"
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
    providerList.find((provider) => provider.name === selectedProviderName) ??
    null

  const [name, setName] = useState("")
  const [kind, setKind] = useState("openai-responses")
  const [apiKey, setApiKey] = useState("")
  const [baseUrl, setBaseUrl] = useState("https://api.openai.com/v1")
  const [models, setModels] = useState<ModelFormRow[]>([emptyModelRow()])
  const [itemQuery, setItemQuery] = useState("")
  const [submitting, setSubmitting] = useState(false)
  const [dirty, setDirty] = useState(false)
  const settingsScopeId = useId()

  const searchInputId = `${settingsScopeId}-search`
  const providerNameInputId = `${settingsScopeId}-provider-name`
  const providerProtocolInputId = `${settingsScopeId}-provider-protocol`
  const providerProtocolLabelId = `${providerProtocolInputId}-label`
  const providerApiKeyInputId = `${settingsScopeId}-provider-api-key`
  const providerBaseUrlInputId = `${settingsScopeId}-provider-base-url`

  useEffect(() => {
    if (!selectedProvider) {
      setName("")
      setKind("openai-responses")
      setApiKey("")
      setBaseUrl("https://api.openai.com/v1")
      setModels([emptyModelRow()])
      setDirty(false)
      return
    }

    setName(selectedProvider.name)
    setKind(selectedProvider.kind)
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
    setDirty(false)
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
    setDirty(true)
  }

  function removeModelRow(index: number) {
    setModels((prev) => prev.filter((_, rowIndex) => rowIndex !== index))
    setDirty(true)
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
    if (value) {
      setKind(value)
      setDirty(true)
    }
  }

  function handleNameChange(value: string) {
    setName(value)
    setDirty(true)
  }

  function handleBaseUrlChange(value: string) {
    setBaseUrl(value)
    setDirty(true)
  }

  function handleApiKeyChange(value: string) {
    setApiKey(value)
    setDirty(true)
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
        setDirty(false)
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
      setDirty(false)
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

  const providerSubmitDisabled =
    submitting ||
    (!selectedProvider && !name.trim()) ||
    !hasValidModel ||
    (!selectedProvider && !apiKey.trim())

  const modelRowsWithId = models.filter((model) => model.id.trim())

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="flex items-center justify-between gap-3 border-b border-border/30 px-4 py-2.5">
        <div className="flex min-w-0 items-center gap-3">
          <button
            type="button"
            onClick={() => setView("chat")}
            className="flex size-8 shrink-0 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
            aria-label="Back to chat"
          >
            <ArrowLeft className="size-3" />
          </button>
          <h1 className="workspace-section-label text-foreground">
            Settings
          </h1>
        </div>

        <div className="flex shrink-0 items-center gap-2">
          <div className="relative min-w-0 sm:w-[200px]">
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
                  ? "Filter providers"
                  : "Filter channels"
              }
              className="h-8 pl-9"
            />
          </div>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden px-4 py-3">
        <div className="mx-auto flex min-h-0 w-full max-w-[1440px] flex-1 flex-col gap-2">
          <div className="grid min-h-0 flex-1 overflow-hidden rounded-xl border border-border/30 bg-card/70 shadow-[var(--workspace-shadow)] xl:grid-cols-[260px_minmax(0,1fr)]">
            <div className="flex min-h-0 flex-col overflow-hidden">
              <div className="shrink-0 border-b border-border/25 px-3 py-2.5">
                <div className="flex items-center justify-between gap-2">
                  <p className="workspace-section-label text-foreground">
                    {isProvidersSection ? "Providers" : "Channels"}
                  </p>
                  <span className={SETTINGS_MONO_COUNT}>
                    {isProvidersSection
                      ? filteredProviders.length
                      : filteredChannels.length}
                  </span>
                </div>
              </div>

              <div className="min-h-0 flex-1 overflow-y-auto p-2.5">
                <div className="space-y-1">
                  {isProvidersSection ? (
                    <>
                      <button
                        type="button"
                        onClick={() => selectProviderName(NEW_PROVIDER_SETTINGS_KEY)}
                        className={cn(
                          "flex w-full items-center gap-2.5 rounded-lg px-3 py-2.5 text-left transition-colors",
                          selectedProviderName === NEW_PROVIDER_SETTINGS_KEY
                            ? "bg-muted/65 text-foreground"
                            : "text-muted-foreground hover:bg-muted/45 hover:text-foreground"
                        )}
                        aria-pressed={selectedProviderName === NEW_PROVIDER_SETTINGS_KEY}
                      >
                        <Plus className="size-3.5 shrink-0" />
                        <span className="text-ui-sm font-medium text-foreground">New provider</span>
                      </button>
                      {filteredProviders.length === 0 ? (
                        <p className="workspace-panel-copy px-3 py-4 text-muted-foreground">
                          {providerList.length === 0 && !normalizedItemQuery
                            ? "No providers yet"
                            : "No matches"}
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
                                "flex w-full items-center gap-2.5 rounded-lg px-3 py-2.5 text-left transition-colors",
                                isActive
                                  ? "bg-muted/65 text-foreground"
                                  : "text-muted-foreground hover:bg-muted/45 hover:text-foreground"
                              )}
                              aria-pressed={isActive}
                            >
                              <span className="size-1.5 shrink-0 rounded-full bg-emerald-500" />
                              <span className="min-w-0 flex-1">
                                <span className="text-ui-sm block truncate font-medium text-foreground">
                                  {providerItem.name}
                                </span>
                                <span className="text-ui-xs mt-0.5 block text-muted-foreground">
                                  {providerItem.models.length} model{providerItem.models.length === 1 ? "" : "s"}
                                </span>
                              </span>
                            </button>
                          )
                        })
                      )}
                    </>
                  ) : channelsLoading && supportedChannels.length === 0 ? (
                    <p className="workspace-panel-copy px-3 py-4 text-muted-foreground">
                      Loading...
                    </p>
                  ) : filteredChannels.length === 0 ? (
                    <p className="workspace-panel-copy px-3 py-4 text-muted-foreground">
                      {supportedChannels.length === 0 && !normalizedItemQuery
                        ? "No channels available"
                        : "No matches"}
                    </p>
                  ) : (
                    filteredChannels.map((channel) => {
                      const isActive = channel.transport === selectedTransport
                      const configured = configuredChannelForTransport(
                        configuredChannels,
                        channel.transport
                      )

                      return (
                        <button
                          key={channel.transport}
                          type="button"
                          onClick={() => selectTransport(channel.transport)}
                          className={cn(
                            "flex w-full items-center gap-2.5 rounded-lg px-3 py-2.5 text-left transition-colors",
                            isActive
                              ? "bg-muted/65 text-foreground"
                              : "text-muted-foreground hover:bg-muted/45 hover:text-foreground"
                          )}
                          aria-pressed={isActive}
                        >
                          <span
                            className={cn(
                              "size-1.5 shrink-0 rounded-full",
                              configured?.enabled
                                ? "bg-emerald-500"
                                : configured
                                  ? "bg-amber-500"
                                  : "bg-muted-foreground/40"
                            )}
                          />
                          <span className="min-w-0 flex-1">
                            <span className="text-ui-sm block truncate font-medium text-foreground">
                              {channel.label}
                            </span>
                            <span className="text-ui-xs mt-0.5 block text-muted-foreground">
                              {configured?.enabled ? "running" : configured ? "paused" : "setup"}
                            </span>
                          </span>
                        </button>
                      )
                    })
                  )}
                </div>
              </div>
            </div>

            <div className="flex min-h-0 flex-col overflow-hidden">
              {isProvidersSection ? (
                <>
                  <div className="shrink-0 border-b border-border/25 px-3 py-2.5">
                    <div className="flex items-center justify-between gap-3">
                      <div className="flex items-center gap-2">
                        {selectedProvider ? (
                          <span className="size-1.5 rounded-full bg-emerald-500" />
                        ) : null}
                        <h2 className="text-ui-sm truncate font-semibold text-foreground">
                          {selectedProvider
                            ? selectedProvider.name
                            : "New provider"}
                        </h2>
                        <Badge variant="outline" className="text-ui-xs">
                          {selectedProvider
                            ? providerProtocolLabel(selectedProvider.kind)
                            : providerProtocolLabel(kind)}
                        </Badge>
                        {selectedProvider ? (
                          <span className="text-ui-xs text-muted-foreground">
                            {selectedProvider.models.length} model{selectedProvider.models.length === 1 ? "" : "s"}
                          </span>
                        ) : null}
                      </div>
                      {selectedProvider ? (
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() => void handleDeleteProvider()}
                          className="h-7 shrink-0 px-2 text-ui-xs text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                        >
                          <Trash2 className="size-3" />
                        </Button>
                      ) : null}
                    </div>
                  </div>

                  <form
                    id="provider-form"
                    onSubmit={handleSubmit}
                    className="flex min-h-0 flex-1 flex-col overflow-hidden"
                  >
                    <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-3">
                      <ProviderConnectionSection
                        name={name}
                        kind={kind}
                        providerNameInputId={providerNameInputId}
                        providerProtocolInputId={providerProtocolInputId}
                        providerProtocolLabelId={providerProtocolLabelId}
                        selectedProviderLocked={selectedProvider != null}
                        onNameChange={handleNameChange}
                        onKindChange={handleKindChange}
                      />

                      <ProviderAuthenticationSection
                        selectedProvider={selectedProvider != null}
                        providerBaseUrlInputId={providerBaseUrlInputId}
                        providerApiKeyInputId={providerApiKeyInputId}
                        baseUrl={baseUrl}
                        apiKey={apiKey}
                        onBaseUrlChange={handleBaseUrlChange}
                        onApiKeyChange={handleApiKeyChange}
                      />

                      <ProviderModelCatalogSection
                        modelRowsWithId={modelRowsWithId.length}
                        models={models}
                        settingsScopeId={settingsScopeId}
                        onAddModel={() => {
                          setModels((prev) => [emptyModelRow(), ...prev])
                          setDirty(true)
                        }}
                        onUpdateModelRow={updateModelRow}
                        onRemoveModelRow={removeModelRow}
                      />
                    </div>

                    <div className="shrink-0 border-t border-border/20 px-3 py-2.5">
                      <div className="flex items-center justify-between gap-3">
                        {dirty ? (
                          <div className="flex items-center gap-2">
                            <span className="text-ui-xs font-medium text-primary">Unsaved</span>
                            <button
                              type="button"
                              onClick={() => {
                                if (selectedProvider) {
                                  setName(selectedProvider.name)
                                  setKind(selectedProvider.kind)
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
                                } else {
                                  setName("")
                                  setKind("openai-responses")
                                  setApiKey("")
                                  setBaseUrl("https://api.openai.com/v1")
                                  setModels([emptyModelRow()])
                                }
                                setDirty(false)
                              }}
                              className="text-ui-xs text-muted-foreground transition-colors hover:text-foreground"
                            >
                              Discard
                            </button>
                          </div>
                        ) : <div />}
                        <Button
                          type="submit"
                          disabled={providerSubmitDisabled}
                          className="min-h-8 min-w-[120px]"
                        >
                          {selectedProvider ? "Save" : "Create"}
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
