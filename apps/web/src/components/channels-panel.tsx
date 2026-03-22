import { useEffect, useId, useMemo, useState, type FormEvent } from "react"
import { ArrowLeft, Trash2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import {
  buildChannelFormState,
  buildDeleteConfirmationCopy,
  collectFieldIssues,
  configuredChannelsForTransport,
  configFieldCount,
  fieldKind,
  fieldLabel,
  requiredFieldKeys,
  schemaProperties,
  summarizeChannelTarget,
  type ChannelFormState,
} from "@/components/channels-panel.helpers"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Switch } from "@/components/ui/switch"
import { useChannelsStore } from "@/stores/channels-store"
import { useChatStore } from "@/stores/chat-store"

export function ChannelsPanel({ embedded = false }: { embedded?: boolean }) {
  const setView = useChatStore((s) => s.setView)
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
  const panelScopeId = useId()

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
    setForm(buildChannelFormState(selectedDefinition, configuredChannel))
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

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setSubmitAttempted(true)
    setSubmitError(null)

    if (!selectedDefinition || missingFieldCount > 0) return

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
    ([, schema]) => fieldKind(schema) !== "boolean"
  )
  const booleanProperties = selectedProperties.filter(
    ([, schema]) => fieldKind(schema) === "boolean"
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
            <p className="text-[10px] font-medium tracking-[0.14em] text-muted-foreground uppercase">
              Channel workspace
            </p>
            <h1 className="mt-0.5 text-sm font-semibold">Settings Workbench</h1>
            <p className="mt-1 text-[12px] leading-5 text-muted-foreground">
              Configure the selected transport, then save a runnable profile.
            </p>
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
          <p className="text-sm font-medium text-foreground">
            No configurable channels available.
          </p>
          <p
            className="mt-2 text-[12px] leading-6 text-muted-foreground"
            role="status"
            aria-live="polite"
          >
            {channelsLoading
              ? "Loading channel catalog..."
              : (channelsError ??
                "The server returned no transports. Check channel registration and server connectivity first.")}
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              size="lg"
              onClick={() => void refresh()}
            >
              Retry loading catalog
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
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0 flex-1">
                <p className="text-[10px] font-medium tracking-[0.14em] text-muted-foreground uppercase">
                  Channel control plane
                </p>
                <div className="flex flex-wrap items-center gap-2">
                  <h2 className="truncate text-[15px] font-semibold">
                    {targetSummary.transportLabel}
                  </h2>
                  <Badge variant="outline" className="text-[10px]">
                    {targetSummary.transportKey}
                  </Badge>
                  <Badge variant="outline" className="text-[10px]">
                    {targetSummary.profileState}
                  </Badge>
                </div>
                <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
                  {selectedDefinition.description ??
                    "This transport bridges external messages into the runtime. Fill the required fields, then save a runnable profile."}
                </p>
              </div>

              <div className="flex min-w-[220px] flex-col items-start gap-2 rounded-lg border border-border/25 bg-muted/[0.16] px-3 py-2.5 sm:items-end">
                <div className="text-left sm:text-right">
                  <p className="text-[10px] font-medium tracking-[0.12em] text-muted-foreground uppercase">
                    Current target
                  </p>
                  <p className="mt-1 text-[13px] font-medium text-foreground">
                    {targetSummary.profileState === "saved"
                      ? `Editing ${targetSummary.profileLabel}`
                      : `Create first profile for ${targetSummary.transportKey}`}
                  </p>
                  <p className="mt-1 text-[11px] text-muted-foreground">
                    {missingFieldCount > 0
                      ? `${missingFieldCount} required field${missingFieldCount === 1 ? " is" : "s are"} still missing`
                      : configuredChannel
                        ? "Ready to update this saved profile"
                        : "Ready to create the first profile"}
                  </p>
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  <label
                    htmlFor={`${panelScopeId}-channel-enabled`}
                    className="text-[11px] font-medium text-foreground"
                  >
                    Runtime enabled
                  </label>
                  <Switch
                    id={`${panelScopeId}-channel-enabled`}
                    checked={form.enabled}
                    onCheckedChange={(checked: boolean) =>
                      setForm((prev) => ({ ...prev, enabled: checked }))
                    }
                    aria-describedby={`${panelScopeId}-channel-enabled-description`}
                    size="default"
                  />
                </div>
              </div>
            </div>

            <div className="mt-2.5 flex flex-wrap items-center gap-1.5 text-[10px] text-muted-foreground">
              <span className="rounded-sm border border-border/30 px-1.5 py-0.5 tabular-nums">
                {configFieldCount(selectedDefinition)} fields
              </span>
              <span className="rounded-sm border border-border/30 px-1.5 py-0.5 tabular-nums">
                {selectedRequired.size} required
              </span>
              <span className="rounded-sm border border-border/30 px-1.5 py-0.5">
                {targetSummary.profileState === "saved"
                  ? `profile ${targetSummary.profileLabel}`
                  : "no saved profile"}
              </span>
              {configuredChannel?.secret_fields_set.length ? (
                <span className="rounded-sm border border-border/30 px-1.5 py-0.5 tabular-nums">
                  {configuredChannel.secret_fields_set.length} secrets set
                </span>
              ) : null}
            </div>

            {matchingChannels.length > 1 ? (
              <div className="mt-2.5 rounded-lg border border-border/30 bg-muted/35 px-3 py-2.5 text-[12px] leading-5 text-foreground/85">
                This transport has multiple saved profiles. The panel is
                currently editing the first stored profile:
                <span className="ml-1 font-medium">
                  {configuredChannel?.id}
                </span>
              </div>
            ) : null}

            <p
              id={`${panelScopeId}-channel-enabled-description`}
              className="mt-2.5 text-[11px] text-muted-foreground"
            >
              Turning runtime off keeps the saved profile but prevents the
              transport worker from starting.
            </p>

            {submitAttempted && missingFieldCount > 0 ? (
              <p
                className="mt-2 text-[12px] font-medium text-destructive"
                role="status"
                aria-live="polite"
              >
                Fill the highlighted required fields before saving.
              </p>
            ) : null}

            {submitError ? (
              <p
                className="mt-2 text-[12px] font-medium text-destructive"
                role="status"
                aria-live="polite"
              >
                {submitError}
              </p>
            ) : null}
          </section>

          <section
            className={
              embedded
                ? "rounded-xl border border-border/30 bg-card/70 p-3 shadow-[var(--workspace-shadow)]"
                : "rounded-2xl border border-border/30 bg-card/70 p-4 shadow-[var(--workspace-shadow)]"
            }
          >
            <div className="mb-2.5">
              <p className="text-[11px] font-medium tracking-[0.12em] text-foreground uppercase">
                Configuration Fields
              </p>
              <p className="mt-1 text-[11px] text-muted-foreground">
                Fill in the connection fields and runtime switches for this
                transport. Saved values become the active transport
                configuration.
              </p>
            </div>

            {selectedProperties.length > 0 ? (
              <div className="space-y-3">
                {nonBooleanProperties.length > 0 ? (
                  <div className="grid gap-2 sm:grid-cols-2">
                    {nonBooleanProperties.map(([key, schema]) => {
                      const kind = fieldKind(schema)
                      const label = fieldLabel(key, schema)
                      const description =
                        typeof schema.description === "string"
                          ? schema.description
                          : null
                      const value = form.config[key]
                      const fieldId = `${panelScopeId}-${selectedDefinition.transport}-${key}`
                      const helperTextId = `${fieldId}-description`
                      const issueId = fieldIssues[key]
                        ? `${fieldId}-issue`
                        : undefined
                      const required = selectedRequired.has(key)
                      const noteParts: string[] = []

                      if (description) {
                        noteParts.push(description)
                      } else if (kind === "url") {
                        noteParts.push(
                          "Used to connect to the transport endpoint. Enter a reachable URL."
                        )
                      } else if (kind === "secret") {
                        noteParts.push(
                          "Used for authenticated access. Prefer a key with limited scope."
                        )
                      } else {
                        noteParts.push(
                          "This value is saved into the current transport configuration."
                        )
                      }

                      if (configuredChannel && kind === "secret") {
                        noteParts.push(
                          "Leave blank to keep the current saved value."
                        )
                      }

                      const helperText = noteParts.join(" ")
                      const describedBy = [helperTextId, issueId]
                        .filter(Boolean)
                        .join(" ")

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
                              <span className="text-[10px] text-muted-foreground">
                                required
                              </span>
                            ) : null}
                          </div>

                          <p id={helperTextId} className="workspace-form-note">
                            {helperText}
                          </p>

                          <Input
                            id={fieldId}
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
                            required={
                              required &&
                              !(configuredChannel && kind === "secret")
                            }
                            aria-describedby={describedBy || undefined}
                            aria-invalid={fieldIssues[key] ? true : undefined}
                            className="h-9 text-[13px]"
                          />

                          {fieldIssues[key] ? (
                            <p
                              id={issueId}
                              className="text-[12px] font-medium text-destructive"
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
                      const description =
                        typeof schema.description === "string"
                          ? schema.description
                          : null
                      const value = form.config[key]
                      const fieldId = `${panelScopeId}-${selectedDefinition.transport}-${key}`
                      const helperTextId = `${fieldId}-description`
                      const issueId = fieldIssues[key]
                        ? `${fieldId}-issue`
                        : undefined
                      const required = selectedRequired.has(key)
                      const helperText =
                        description ??
                        "This switch affects how the transport runs."
                      const describedBy = [helperTextId, issueId]
                        .filter(Boolean)
                        .join(" ")

                      return (
                        <div
                          key={key}
                          className="flex items-start justify-between gap-3"
                        >
                          <div className="space-y-1.5 pr-4">
                            <div className="flex flex-wrap items-center gap-1.5">
                              <label
                                htmlFor={fieldId}
                                className="workspace-form-label"
                              >
                                {label}
                              </label>
                              {required ? (
                                <span className="text-[10px] text-muted-foreground">
                                  required
                                </span>
                              ) : null}
                            </div>
                            <p
                              id={helperTextId}
                              className="workspace-form-note"
                            >
                              {helperText}
                            </p>
                            {fieldIssues[key] ? (
                              <p
                                id={issueId}
                                className="text-[12px] font-medium text-destructive"
                              >
                                {fieldIssues[key]}
                              </p>
                            ) : null}
                          </div>

                          <Switch
                            id={fieldId}
                            checked={value === true}
                            onCheckedChange={(checked: boolean) =>
                              updateConfigField(key, checked)
                            }
                            aria-describedby={describedBy || undefined}
                            size="default"
                          />
                        </div>
                      )
                    })}
                  </div>
                ) : null}
              </div>
            ) : (
              <p className="text-[12px] leading-5 text-muted-foreground">
                This transport exposes no editable fields. You can still control
                its runtime state with the switch below.
              </p>
            )}
          </section>

          {configuredChannel ? (
            <section
              className={
                embedded
                  ? "rounded-xl border border-destructive/30 bg-destructive/[0.04] p-3 shadow-[var(--workspace-shadow)]"
                  : "rounded-2xl border border-destructive/30 bg-destructive/[0.04] p-4 shadow-[var(--workspace-shadow)]"
              }
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <p className="text-[11px] font-medium tracking-[0.12em] text-destructive uppercase">
                    Danger Zone
                  </p>
                  <p className="mt-1 text-[12px] leading-5 text-destructive/85">
                    Remove the saved profile for this transport. You will need
                    to recreate it before the worker can run again.
                  </p>
                </div>

                {deleteConfirming ? (
                  <div className="flex flex-wrap items-center justify-end gap-2">
                    <p className="text-[12px] font-medium text-destructive">
                      {buildDeleteConfirmationCopy(targetSummary)}
                    </p>
                    <Button
                      type="button"
                      variant="outline"
                      size="lg"
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
                      size="lg"
                      disabled={deletePending}
                      onClick={() => void handleDelete()}
                    >
                      {deletePending ? "Deleting..." : "Confirm delete"}
                    </Button>
                  </div>
                ) : (
                  <Button
                    type="button"
                    variant="ghost"
                    size="lg"
                    className="shrink-0 text-destructive hover:bg-destructive/10 hover:text-destructive"
                    onClick={() => setDeleteConfirming(true)}
                  >
                    <Trash2 className="size-3.5" />
                    Delete profile
                  </Button>
                )}
              </div>

              {deleteError ? (
                <p
                  className="mt-3 text-[12px] font-medium text-destructive"
                  role="status"
                  aria-live="polite"
                >
                  {deleteError}
                </p>
              ) : null}
            </section>
          ) : null}

          <section
            className={
              embedded
                ? "rounded-xl border border-border/30 bg-card/70 px-3 py-2.5 shadow-[var(--workspace-shadow)]"
                : "rounded-2xl border border-border/30 bg-card/70 px-4 py-3 shadow-[var(--workspace-shadow)]"
            }
          >
            <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
              <p className="text-[11px] text-muted-foreground">
                {missingFieldCount > 0
                  ? `${missingFieldCount} required field${missingFieldCount === 1 ? " is" : "s are"} still missing`
                  : configuredChannel
                    ? "Submitting will update the current saved profile."
                    : "Submitting will create the first profile for this transport."}
              </p>
              <Button
                type="submit"
                disabled={submitting || !canSubmit}
                className="min-h-9 min-w-[190px]"
              >
                {configuredChannel ? "Save profile" : "Create profile"}
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
