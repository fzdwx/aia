import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useState,
  type FormEvent,
} from "react"
import { ArrowLeft, Eye, EyeOff, Trash2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import {
  buildChannelFormState,
  buildDeleteConfirmationCopy,
  collectFieldIssues,
  configuredChannelsForTransport,
  fieldKind,
  fieldLabel,
  requiredFieldKeys,
  schemaProperties,
  summarizeChannelTarget,
  type ChannelFormState,
} from "./helpers"
import { WeixinLoginPanel } from "./weixin-login"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Switch } from "@/components/ui/switch"
import { useChannelsStore } from "@/stores/channels-store"
import { useWorkbenchStore } from "@/stores/workbench-store"
import { cn } from "@/lib/utils"

export function ChannelsPanel({ embedded = false }: { embedded?: boolean }) {
  const setView = useWorkbenchStore((s) => s.setView)
  const initializeChannels = useChannelsStore((s) => s.initialize)
  const refresh = useChannelsStore((s) => s.refresh)
  const supportedChannels = useChannelsStore((s) => s.supportedChannels)
  const configuredChannels = useChannelsStore((s) => s.configuredChannels)
  const selectedTransport = useChannelsStore((s) => s.selectedTransport)
  const channelsLoading = useChannelsStore((s) => s.loading)
  const channelsError = useChannelsStore((s) => s.error)
  const createChannel = useChannelsStore((s) => s.createChannel)
  const updateChannel = useChannelsStore((s) => s.updateChannel)
  const deleteChannel = useChannelsStore((s) => s.deleteChannel)

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
  const [form, setForm] = useState<ChannelFormState>(() =>
    buildChannelFormState(selectedDefinition, configuredChannel)
  )
  const [submitting, setSubmitting] = useState(false)
  const [submitAttempted, setSubmitAttempted] = useState(false)
  const [submitError, setSubmitError] = useState<string | null>(null)
  const [deleteConfirming, setDeleteConfirming] = useState(false)
  const [deletePending, setDeletePending] = useState(false)
  const [deleteError, setDeleteError] = useState<string | null>(null)
  const [visibleSecrets, setVisibleSecrets] = useState<Set<string>>(new Set())
  const panelScopeId = useId()
  const isWeixinTransport = selectedDefinition?.transport === "weixin"
  const hasBotTokenField = useMemo(
    () => selectedProperties.some(([key]) => key === "bot_token"),
    [selectedProperties]
  )
  const hasTokenField = useMemo(
    () => selectedProperties.some(([key]) => key === "token"),
    [selectedProperties]
  )
  const canWriteBotToken =
    hasBotTokenField ||
    Object.prototype.hasOwnProperty.call(form.config, "bot_token")
  const canWriteToken =
    hasTokenField || Object.prototype.hasOwnProperty.call(form.config, "token")
  const isWeixinHiddenField = useCallback(
    (key: string) =>
      isWeixinTransport &&
      ["bot_token", "token", "account_id", "user_id"].includes(key),
    [isWeixinTransport]
  )

  const persistChannelProfile = useCallback(
    async ({
      config,
      enabled,
    }: {
      config: Record<string, unknown>
      enabled: boolean
    }) => {
      if (!selectedDefinition) return

      if (configuredChannel) {
        await updateChannel(configuredChannel.id, {
          enabled,
          config,
        })
        return
      }

      await createChannel({
        id: selectedDefinition.transport,
        name: selectedDefinition.label,
        transport: selectedDefinition.transport,
        enabled,
        config,
      })
    },
    [configuredChannel, createChannel, selectedDefinition, updateChannel]
  )

  useEffect(() => {
    void initializeChannels().catch(() => {})
  }, [initializeChannels])

  const targetSummary = useMemo(
    () =>
      summarizeChannelTarget(
        selectedDefinition,
        configuredChannel,
        matchingChannels
      ),
    [configuredChannel, matchingChannels, selectedDefinition]
  )
  const fieldIssues = useMemo(
    () =>
      collectFieldIssues(
        selectedDefinition,
        form.config,
        Boolean(configuredChannel)
      ),
    [configuredChannel, form.config, selectedDefinition]
  )
  const missingFieldCount = Object.keys(fieldIssues).length

  useEffect(() => {
    const nextForm = buildChannelFormState(
      selectedDefinition,
      configuredChannel
    )
    setForm(nextForm)
    setSubmitAttempted(false)
    setSubmitError(null)
    setDeleteConfirming(false)
    setDeleteError(null)
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

  function toggleSecretVisibility(key: string) {
    setVisibleSecrets((prev) => {
      const next = new Set(prev)
      if (next.has(key)) {
        next.delete(key)
      } else {
        next.add(key)
      }
      return next
    })
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setSubmitAttempted(true)
    setSubmitError(null)

    if (!selectedDefinition || missingFieldCount > 0) return

    setSubmitting(true)

    try {
      await persistChannelProfile({
        config: form.config,
        enabled: form.enabled,
      })
    } catch (error) {
      setSubmitError(
        error instanceof Error
          ? error.message
          : "Failed to save channel profile"
      )
    } finally {
      setSubmitting(false)
    }
  }

  async function handleDelete() {
    if (!configuredChannel) return
    setDeletePending(true)
    setDeleteError(null)

    try {
      await deleteChannel(configuredChannel.id)
      setDeleteConfirming(false)
    } catch (error) {
      setDeleteError(
        error instanceof Error ? error.message : "Failed to delete profile"
      )
    } finally {
      setDeletePending(false)
    }
  }

  const canSubmit = Boolean(selectedDefinition) && missingFieldCount === 0
  const nonBooleanProperties = selectedProperties.filter(
    ([key, schema]) =>
      !isWeixinHiddenField(key) && fieldKind(schema) !== "boolean"
  )
  const booleanProperties = selectedProperties.filter(
    ([key, schema]) =>
      !isWeixinHiddenField(key) && fieldKind(schema) === "boolean"
  )

  const content = (
    <div
      className={
        embedded
          ? "space-y-3"
          : "mx-auto max-w-[920px] px-4 py-6 sm:px-6 sm:py-8"
      }
    >
      {!embedded ? (
        <div className="mb-4 flex items-start gap-3 border-b border-border/25 pb-3">
          <Button
            onClick={() => setView("chat")}
            variant="ghost"
            size="icon-lg"
            className="mt-0.5 shrink-0 text-muted-foreground hover:text-foreground"
            aria-label="Back to chat"
          >
            <ArrowLeft className="size-3" />
          </Button>

          <div>
            <p className="workspace-section-label text-muted-foreground">
              Channel workspace
            </p>
            <h1 className="workspace-panel-title mt-0.5">Settings Workbench</h1>
          </div>
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
          <p className="workspace-panel-title text-foreground">
            No configurable channels available.
          </p>
          <p
            className="workspace-panel-copy mt-2 text-muted-foreground"
            role="status"
            aria-live="polite"
          >
            {channelsLoading
              ? "Loading..."
              : (channelsError ??
                "No transports found.")}
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void refresh()}
            >
              Retry
            </Button>
          </div>
        </section>
      ) : (
        <form
          onSubmit={handleSubmit}
          className={embedded ? "space-y-3" : "space-y-4"}
        >
          <section
            className={
              embedded
                ? "rounded-xl border border-border/30 bg-card/70 p-3 shadow-[var(--workspace-shadow)]"
                : "rounded-2xl border border-border/30 bg-card/70 p-4 shadow-[var(--workspace-shadow)]"
            }
          >
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <span
                  className={cn(
                    "size-1.5 shrink-0 rounded-full",
                    configuredChannel?.enabled
                      ? "bg-emerald-500"
                      : configuredChannel
                        ? "bg-amber-500"
                        : "bg-muted-foreground/40"
                  )}
                />
                <h2 className="text-ui-sm truncate font-semibold text-foreground">
                  {targetSummary.transportLabel}
                </h2>
                <Badge variant="outline" className="text-ui-xs">
                  {targetSummary.transportKey}
                </Badge>
                {configuredChannel ? (
                  <span className="text-ui-xs text-muted-foreground">
                    {configuredChannel.enabled ? "running" : "paused"}
                  </span>
                ) : null}
              </div>

              <div className="flex items-center gap-2">
                <div className="flex items-center gap-2">
                  <Switch
                    id={`${panelScopeId}-channel-enabled`}
                    checked={form.enabled}
                    onCheckedChange={(checked: boolean) =>
                      setForm((prev) => ({ ...prev, enabled: checked }))
                    }
                    size="default"
                    aria-label="Runtime enabled"
                  />
                  <label
                    htmlFor={`${panelScopeId}-channel-enabled`}
                    className="workspace-form-label cursor-default"
                  >
                    Enabled
                  </label>
                </div>
                {configuredChannel ? (
                  deleteConfirming ? (
                    <div className="flex items-center gap-1.5">
                      <span className="text-ui-xs text-destructive">
                        {buildDeleteConfirmationCopy(targetSummary)}
                      </span>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="h-7 px-2"
                        onClick={() => {
                          setDeleteConfirming(false)
                          setDeleteError(null)
                        }}
                      >
                        Cancel
                      </Button>
                      <Button
                        type="button"
                        variant="destructive"
                        size="sm"
                        className="h-7 px-2"
                        disabled={deletePending}
                        onClick={() => void handleDelete()}
                      >
                        {deletePending ? "Deleting..." : "Delete"}
                      </Button>
                    </div>
                  ) : (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="h-7 px-2 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                      onClick={() => setDeleteConfirming(true)}
                    >
                      <Trash2 className="size-3" />
                    </Button>
                  )
                ) : null}
              </div>
            </div>

            {submitAttempted && missingFieldCount > 0 ? (
              <p
                className="text-ui-xs mt-2 font-medium text-destructive"
                role="status"
                aria-live="polite"
              >
                {missingFieldCount} required field{missingFieldCount === 1 ? "" : "s"} missing
              </p>
            ) : null}

            {submitError ? (
              <p
                className="text-ui-xs mt-2 font-medium text-destructive"
                role="status"
                aria-live="polite"
              >
                {submitError}
              </p>
            ) : null}

            {deleteError ? (
              <p
                className="text-ui-xs mt-2 font-medium text-destructive"
                role="status"
                aria-live="polite"
              >
                {deleteError}
              </p>
            ) : null}
          </section>

          {isWeixinTransport && selectedDefinition ? (
            <WeixinLoginPanel
              embedded={embedded}
              definition={selectedDefinition}
              configuredChannel={configuredChannel}
              config={form.config}
              canWriteBotToken={canWriteBotToken}
              canWriteToken={canWriteToken}
              onApplyConfigPatch={(patch) => {
                setForm((prev) => ({
                  ...prev,
                  config: {
                    ...prev.config,
                    ...patch,
                  },
                }))
              }}
              onPersistConfig={(nextConfig) =>
                persistChannelProfile({
                  config: nextConfig,
                  enabled: form.enabled,
                })
              }
            />
          ) : null}

          <section
            className={
              embedded
                ? "rounded-xl border border-border/30 bg-card/70 p-3 shadow-[var(--workspace-shadow)]"
                : "rounded-2xl border border-border/30 bg-card/70 p-4 shadow-[var(--workspace-shadow)]"
            }
          >
            <p className="workspace-section-label text-foreground">
              Configuration
            </p>

            {selectedProperties.length > 0 ? (
              <div className="mt-2.5 space-y-3">
                {nonBooleanProperties.length > 0 ? (
                  <div className="grid gap-2 sm:grid-cols-2">
                    {nonBooleanProperties.map(([key, schema]) => {
                      const kind = fieldKind(schema)
                      const label = fieldLabel(key, schema)
                      const value = form.config[key]
                      const fieldId = `${panelScopeId}-${selectedDefinition.transport}-${key}`
                      const issueId = fieldIssues[key]
                        ? `${fieldId}-issue`
                        : undefined
                      const required = selectedRequired.has(key)
                      const isSecret = kind === "secret"
                      const isSecretVisible = visibleSecrets.has(key)

                      return (
                        <div key={key} className="space-y-1.5">
                          <div className="flex items-center gap-1.5">
                            <label
                              htmlFor={fieldId}
                              className="workspace-form-label"
                            >
                              {label}
                            </label>
                            {required ? (
                              <span className="text-ui-xs text-muted-foreground">
                                required
                              </span>
                            ) : null}
                          </div>

                          {isSecret ? (
                            <div className="relative">
                              <Input
                                id={fieldId}
                                type={isSecretVisible ? "text" : "password"}
                                value={typeof value === "string" ? value : ""}
                                onChange={(event) =>
                                  updateConfigField(key, event.target.value)
                                }
                                placeholder={
                                  configuredChannel
                                    ? "Leave blank to keep current value"
                                    : undefined
                                }
                                required={required && !(configuredChannel && isSecret)}
                                aria-invalid={fieldIssues[key] ? true : undefined}
                                aria-describedby={issueId || undefined}
                                className="h-8 pr-9"
                              />
                              <button
                                type="button"
                                onClick={() => toggleSecretVisibility(key)}
                                className="absolute top-0 right-0 flex size-8 items-center justify-center text-muted-foreground/60 transition-colors hover:text-foreground"
                                aria-label={isSecretVisible ? "Hide" : "Show"}
                                tabIndex={-1}
                              >
                                {isSecretVisible ? <EyeOff className="size-3.5" /> : <Eye className="size-3.5" />}
                              </button>
                            </div>
                          ) : (
                            <Input
                              id={fieldId}
                              type={kind === "url" ? "url" : "text"}
                              value={typeof value === "string" ? value : ""}
                              onChange={(event) =>
                                updateConfigField(key, event.target.value)
                              }
                              placeholder={
                                typeof schema.default === "string"
                                  ? schema.default
                                  : undefined
                              }
                              required={required}
                              aria-invalid={fieldIssues[key] ? true : undefined}
                              aria-describedby={issueId || undefined}
                              className="h-8"
                            />
                          )}

                          {fieldIssues[key] ? (
                            <p
                              id={issueId}
                              className="text-ui-xs font-medium text-destructive"
                            >
                              {fieldIssues[key]}
                            </p>
                          ) : null}
                        </div>
                      )
                    })}
                  </div>
                ) : null}

                {booleanProperties.length > 0 ? (
                  <div
                    className={
                      nonBooleanProperties.length > 0
                        ? "space-y-2 border-t border-border/20 pt-2.5"
                        : "space-y-2"
                    }
                  >
                    {booleanProperties.map(([key, schema]) => {
                      const label = fieldLabel(key, schema)
                      const value = form.config[key]
                      const fieldId = `${panelScopeId}-${selectedDefinition.transport}-${key}`
                      const issueId = fieldIssues[key]
                        ? `${fieldId}-issue`
                        : undefined
                      const required = selectedRequired.has(key)

                      return (
                        <div
                          key={key}
                          className="flex items-center justify-between gap-3"
                        >
                          <div className="flex items-center gap-1.5">
                            <label
                              htmlFor={fieldId}
                              className="workspace-form-label"
                            >
                              {label}
                            </label>
                            {required ? (
                              <span className="text-ui-xs text-muted-foreground">
                                required
                              </span>
                            ) : null}
                          </div>
                          <Switch
                            id={fieldId}
                            checked={value === true}
                            onCheckedChange={(checked: boolean) =>
                              updateConfigField(key, checked)
                            }
                            aria-describedby={issueId || undefined}
                            size="default"
                          />
                        </div>
                      )
                    })}
                  </div>
                ) : null}
              </div>
            ) : (
              <p className="workspace-meta mt-2 text-muted-foreground">
                No editable fields.
              </p>
            )}
          </section>

          <div className="flex items-center justify-end">
            <Button
              type="submit"
              disabled={submitting || !canSubmit}
              className="min-h-8 min-w-[120px]"
            >
              {configuredChannel ? "Save" : "Create"}
            </Button>
          </div>
        </form>
      )}
    </div>
  )

  if (embedded) {
    return content
  }

  return <ScrollArea className="min-h-0 flex-1">{content}</ScrollArea>
}
