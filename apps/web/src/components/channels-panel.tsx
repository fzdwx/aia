import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from "react"
import { ArrowLeft, Trash2 } from "lucide-react"
import QRCode from "qrcode"

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

const WEIXIN_POLL_INTERVAL_MS = 1800
const WEIXIN_POLL_MAX_ATTEMPTS = 120
const WEIXIN_QR_RENDER_SIZE = 288

type WeixinQrResponse = {
  qrcode: string
  qrcode_url?: string | null
}

type WeixinLoginStatusResponse = {
  status?: string
  bot_token?: string | null
  account_id?: string | null
  user_id?: string | null
}

type WeixinPollingState =
  | "idle"
  | "requesting_qr"
  | "waiting_scan"
  | "polling"
  | "success"
  | "expired"
  | "failed"

function normalizeWeixinStatus(value: unknown): string {
  if (typeof value !== "string") return "wait"
  const normalized = value.trim().toLowerCase()
  return normalized.length > 0 ? normalized : "wait"
}

function isWeixinExpiredStatus(status: string): boolean {
  return ["expired", "timeout", "timed_out", "qrcode_expired"].includes(status)
}

function isWeixinRejectedStatus(status: string): boolean {
  return ["cancel", "cancelled", "denied", "rejected", "failed"].includes(
    status
  )
}

function weixinPollingCopy(status: string): string {
  if (["wait", "waiting", "pending"].includes(status)) {
    return "QR code ready. Waiting for a scan."
  }
  if (["scan", "scanned", "confirming", "authorizing"].includes(status)) {
    return "Scanned. Waiting for confirmation on the phone."
  }
  return `Polling: ${status}`
}

function resolveWeixinQrImmediateSrc(rawUrl: string | null): string | null {
  if (!rawUrl) return null
  const trimmed = rawUrl.trim()
  if (trimmed.length === 0) return null
  if (trimmed.startsWith("data:image/")) {
    return trimmed
  }
  if (trimmed.startsWith("https://") || trimmed.startsWith("http://")) {
    try {
      const parsed = new URL(trimmed)
      const pathname = parsed.pathname.toLowerCase()
      const explicitImageFormat =
        parsed.searchParams.get("format")?.toLowerCase() ?? ""
      const looksLikeImageUrl =
        /\.(png|jpe?g|gif|webp|bmp|svg|ico)$/.test(pathname) ||
        ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico"].includes(
          explicitImageFormat
        )

      if (looksLikeImageUrl) {
        return trimmed
      }
    } catch {
      return null
    }

    return null
  }
  return `data:image/png;base64,${trimmed}`
}

async function readResponseError(
  response: Response,
  fallback: string
): Promise<string> {
  try {
    const payload = (await response.json()) as Record<string, unknown>
    if (typeof payload.error === "string" && payload.error.trim().length > 0) {
      return payload.error
    }
    if (
      typeof payload.message === "string" &&
      payload.message.trim().length > 0
    ) {
      return payload.message
    }
  } catch {
    return fallback
  }
  return fallback
}

async function requestWeixinLoginQr(): Promise<WeixinQrResponse> {
  const response = await fetch("/api/channels/weixin/login/qr", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({}),
  })
  if (!response.ok) {
    throw new Error(
      await readResponseError(
        response,
        `POST /api/channels/weixin/login/qr failed: ${response.status}`
      )
    )
  }
  return (await response.json()) as WeixinQrResponse
}

async function requestWeixinLoginStatus(
  qrcode: string
): Promise<WeixinLoginStatusResponse> {
  const response = await fetch("/api/channels/weixin/login/status", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ qrcode }),
  })
  if (!response.ok) {
    throw new Error(
      await readResponseError(
        response,
        `POST /api/channels/weixin/login/status failed: ${response.status}`
      )
    )
  }
  return (await response.json()) as WeixinLoginStatusResponse
}

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
  const [weixinQrCode, setWeixinQrCode] = useState<string | null>(null)
  const [weixinQrImageUrl, setWeixinQrImageUrl] = useState<string | null>(null)
  const [weixinPollingState, setWeixinPollingState] =
    useState<WeixinPollingState>("idle")
  const [weixinPollingStatus, setWeixinPollingStatus] = useState(
    "QR login has not started yet."
  )
  const [weixinRawStatus, setWeixinRawStatus] = useState<string | null>(null)
  const [weixinPollingAttempts, setWeixinPollingAttempts] = useState(0)
  const [weixinPollingEnabled, setWeixinPollingEnabled] = useState(false)
  const [weixinScanError, setWeixinScanError] = useState<string | null>(null)
  const [weixinSaveHint, setWeixinSaveHint] = useState<string | null>(null)
  const [weixinLinkedAccountId, setWeixinLinkedAccountId] = useState<
    string | null
  >(null)
  const [weixinLinkedUserId, setWeixinLinkedUserId] = useState<string | null>(
    null
  )
  const weixinPollingAttemptsRef = useRef(0)
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
  const [weixinQrImageSrc, setWeixinQrImageSrc] = useState<string | null>(null)
  const hasWeixinQrTicket = Boolean(weixinQrCode)
  const hasWeixinQrImage = Boolean(weixinQrImageSrc)
  const weixinQrPanelState = useMemo<
    "loading" | "empty" | "error" | "success"
  >(() => {
    if (weixinPollingState === "requesting_qr") {
      return "loading"
    }
    if (hasWeixinQrTicket || hasWeixinQrImage) {
      return "success"
    }
    if (weixinPollingState === "failed") {
      return "error"
    }
    return "empty"
  }, [hasWeixinQrImage, hasWeixinQrTicket, weixinPollingState])

  const persistWeixinLogin = useCallback(
    async (nextConfig: Record<string, unknown>) => {
      if (!selectedDefinition) return

      if (configuredChannel) {
        await updateChannel(configuredChannel.id, {
          enabled: form.enabled,
          config: nextConfig,
        })
        return
      }

      await createChannel({
        id: selectedDefinition.transport,
        name: selectedDefinition.label,
        transport: selectedDefinition.transport,
        enabled: form.enabled,
        config: nextConfig,
      })
    },
    [
      configuredChannel,
      createChannel,
      form.enabled,
      selectedDefinition,
      updateChannel,
    ]
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
    let cancelled = false

    const source = weixinQrImageUrl?.trim() ?? ""
    if (source.length === 0) {
      setWeixinQrImageSrc(null)
      return () => {
        cancelled = true
      }
    }

    const immediate = resolveWeixinQrImmediateSrc(source)
    if (immediate) {
      setWeixinQrImageSrc(immediate)
      return () => {
        cancelled = true
      }
    }

    QRCode.toDataURL(source, {
      width: WEIXIN_QR_RENDER_SIZE,
      margin: 1,
      errorCorrectionLevel: "M",
    })
      .then((dataUrl: string) => {
        if (!cancelled) {
          setWeixinQrImageSrc(dataUrl)
        }
      })
      .catch(() => {
        if (!cancelled) {
          setWeixinQrImageSrc(null)
        }
      })

    return () => {
      cancelled = true
    }
  }, [weixinQrImageUrl])

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

    const preserveWeixinScanState =
      selectedDefinition?.transport === "weixin" &&
      (weixinPollingState === "success" ||
        weixinPollingState === "polling" ||
        weixinPollingState === "waiting_scan" ||
        weixinPollingState === "requesting_qr" ||
        weixinPollingEnabled ||
        weixinQrCode !== null ||
        weixinQrImageUrl !== null)

    if (!preserveWeixinScanState) {
      setWeixinQrCode(null)
      setWeixinQrImageUrl(null)
      setWeixinPollingState("idle")
      setWeixinPollingStatus("QR login has not started yet.")
      setWeixinRawStatus(null)
      setWeixinPollingAttempts(0)
      setWeixinPollingEnabled(false)
      weixinPollingAttemptsRef.current = 0
      setWeixinScanError(null)
      setWeixinSaveHint(null)
      setWeixinLinkedAccountId(
        typeof nextForm.config.account_id === "string" &&
          nextForm.config.account_id.trim().length > 0
          ? nextForm.config.account_id.trim()
          : null
      )
      setWeixinLinkedUserId(
        typeof nextForm.config.user_id === "string" &&
          nextForm.config.user_id.trim().length > 0
          ? nextForm.config.user_id.trim()
          : null
      )
    }
  }, [
    configuredChannel,
    selectedDefinition,
    weixinPollingEnabled,
    weixinPollingState,
    weixinQrCode,
    weixinQrImageUrl,
  ])

  function updateConfigField(key: string, value: unknown) {
    setForm((prev) => ({
      ...prev,
      config: {
        ...prev.config,
        [key]: value,
      },
    }))
  }

  async function handleStartWeixinScanLogin() {
    if (!isWeixinTransport) return

    setWeixinScanError(null)
    setWeixinSaveHint(null)
    setWeixinLinkedUserId(null)
    setWeixinRawStatus(null)
    setWeixinPollingAttempts(0)
    setWeixinPollingEnabled(false)
    weixinPollingAttemptsRef.current = 0
    setWeixinPollingState("requesting_qr")
    setWeixinPollingStatus("Requesting a login QR code…")

    try {
      const payload = await requestWeixinLoginQr()
      const nextQrCode =
        typeof payload.qrcode === "string" ? payload.qrcode.trim() : ""
      const nextQrImageUrl =
        typeof payload.qrcode_url === "string" ? payload.qrcode_url.trim() : ""

      if (!nextQrCode && nextQrImageUrl.length === 0) {
        throw new Error(
          "The login response returned neither a QR ticket nor a renderable QR image."
        )
      }

      setWeixinQrCode(nextQrCode.length > 0 ? nextQrCode : null)
      setWeixinQrImageUrl(nextQrImageUrl.length > 0 ? nextQrImageUrl : null)
      if (nextQrCode.length > 0) {
        setWeixinPollingState("waiting_scan")
        setWeixinPollingStatus(
          "QR code generated. Scan it with Weixin and confirm on your phone."
        )
        setWeixinPollingEnabled(true)
      } else {
        setWeixinPollingState("success")
        setWeixinPollingStatus(
          "QR image is available, but no polling ticket was returned. Scan it manually and refresh if needed."
        )
        setWeixinPollingEnabled(false)
      }
    } catch (error) {
      setWeixinPollingState("failed")
      setWeixinPollingStatus("Failed to request a QR code. Try again.")
      setWeixinScanError(
        error instanceof Error ? error.message : "Failed to request weixin QR"
      )
    }
  }

  function handleStopWeixinPolling() {
    setWeixinPollingEnabled(false)
    if (weixinPollingState !== "success") {
      setWeixinPollingState("idle")
      setWeixinPollingStatus(
        "Polling stopped. Request a new QR code when ready."
      )
    }
  }

  useEffect(() => {
    if (!isWeixinTransport || !weixinPollingEnabled || !weixinQrCode) return

    let disposed = false
    let timeoutId: ReturnType<typeof setTimeout> | null = null

    const poll = async () => {
      if (disposed) return
      setWeixinPollingState("polling")

      try {
        const statusPayload = await requestWeixinLoginStatus(weixinQrCode)
        if (disposed) return

        const status = normalizeWeixinStatus(statusPayload.status)
        const botToken =
          typeof statusPayload.bot_token === "string"
            ? statusPayload.bot_token.trim()
            : ""
        const accountId =
          typeof statusPayload.account_id === "string"
            ? statusPayload.account_id.trim()
            : ""
        const linkedUserId =
          typeof statusPayload.user_id === "string"
            ? statusPayload.user_id.trim()
            : ""
        setWeixinRawStatus(status)
        weixinPollingAttemptsRef.current += 1
        setWeixinPollingAttempts(weixinPollingAttemptsRef.current)

        if (botToken.length > 0) {
          const updatePatch: Record<string, unknown> = {}
          if (canWriteBotToken) {
            updatePatch.bot_token = botToken
          }
          if (canWriteToken) {
            updatePatch.token = botToken
          }
          if (accountId.length > 0) {
            updatePatch.account_id = accountId
          }
          if (linkedUserId.length > 0) {
            updatePatch.user_id = linkedUserId
          }
          if (Object.keys(updatePatch).length > 0) {
            const nextConfig = {
              ...form.config,
              ...updatePatch,
            }
            setForm((prev) => ({
              ...prev,
              config: {
                ...prev.config,
                ...updatePatch,
              },
            }))

            try {
              await persistWeixinLogin(nextConfig)
            } catch (error) {
              setWeixinPollingEnabled(false)
              setWeixinPollingState("failed")
              setWeixinPollingStatus(
                "Login succeeded, but saving the profile failed."
              )
              setWeixinScanError(
                error instanceof Error
                  ? error.message
                  : "Failed to persist the Weixin profile"
              )
              setWeixinSaveHint(
                "The login values are still in the form. Use Save profile after fixing the error."
              )
              return
            }
          }

          setWeixinPollingEnabled(false)
          setWeixinPollingState("success")
          setWeixinLinkedAccountId(accountId.length > 0 ? accountId : null)
          setWeixinLinkedUserId(linkedUserId.length > 0 ? linkedUserId : null)
          setWeixinPollingStatus(
            "Login confirmed. The Weixin profile is ready to use."
          )
          setWeixinSaveHint(
            "The profile was updated automatically. You can keep using the configuration below."
          )
          setWeixinScanError(null)
          return
        }

        if (isWeixinExpiredStatus(status)) {
          setWeixinPollingEnabled(false)
          setWeixinPollingState("expired")
          setWeixinPollingStatus("This QR code expired. Request a new one.")
          return
        }

        if (isWeixinRejectedStatus(status)) {
          setWeixinPollingEnabled(false)
          setWeixinPollingState("failed")
          setWeixinPollingStatus(
            "The login was cancelled or rejected. Scan again to continue."
          )
          return
        }

        if (weixinPollingAttemptsRef.current >= WEIXIN_POLL_MAX_ATTEMPTS) {
          setWeixinPollingEnabled(false)
          setWeixinPollingState("expired")
          setWeixinPollingStatus("Polling timed out. Request a new QR code.")
          return
        }

        setWeixinPollingStatus(weixinPollingCopy(status))
        timeoutId = setTimeout(poll, WEIXIN_POLL_INTERVAL_MS)
      } catch (error) {
        if (disposed) return
        setWeixinPollingEnabled(false)
        setWeixinPollingState("failed")
        setWeixinPollingStatus("Polling failed. Request a new QR code.")
        setWeixinScanError(
          error instanceof Error
            ? error.message
            : "Failed to poll weixin login status"
        )
      }
    }

    void poll()

    return () => {
      disposed = true
      if (timeoutId) {
        clearTimeout(timeoutId)
      }
    }
  }, [
    canWriteBotToken,
    canWriteToken,
    form.config,
    isWeixinTransport,
    persistWeixinLogin,
    weixinPollingEnabled,
    weixinQrCode,
  ])

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

          {isWeixinTransport ? (
            <section
              className={
                embedded
                  ? "rounded-xl border border-border/30 bg-card/70 p-3 shadow-[var(--workspace-shadow)]"
                  : "rounded-2xl border border-border/30 bg-card/70 p-4 shadow-[var(--workspace-shadow)]"
              }
            >
              <div className="mb-2.5">
                <div className="flex flex-wrap items-center gap-2">
                  <p className="text-[11px] font-medium tracking-[0.12em] text-foreground uppercase">
                    Weixin QR Login
                  </p>
                  <Badge variant="outline" className="text-[10px]">
                    {weixinPollingState.replaceAll("_", " ")}
                  </Badge>
                  {weixinRawStatus ? (
                    <Badge variant="outline" className="text-[10px]">
                      status: {weixinRawStatus}
                    </Badge>
                  ) : null}
                </div>
                <p className="mt-1 text-[11px] text-muted-foreground">
                  Start Weixin QR login directly from this workbench. Once the
                  scan succeeds, the form is filled with the returned token and
                  saved automatically.
                </p>
              </div>

              <div className="flex flex-wrap items-end gap-2">
                <Button
                  type="button"
                  size="lg"
                  onClick={() => void handleStartWeixinScanLogin()}
                  disabled={
                    weixinPollingState === "requesting_qr" ||
                    weixinPollingEnabled
                  }
                >
                  {weixinPollingState === "requesting_qr"
                    ? "Getting QR..."
                    : weixinQrCode
                      ? "Refresh QR"
                      : "Get QR code"}
                </Button>

                {weixinPollingEnabled ? (
                  <Button
                    type="button"
                    variant="outline"
                    size="lg"
                    onClick={handleStopWeixinPolling}
                  >
                    Stop polling
                  </Button>
                ) : null}
              </div>

              {weixinQrPanelState === "loading" ? (
                <div className="mt-3 rounded-lg border border-border/25 bg-muted/[0.16] p-3">
                  <p
                    className="text-[12px] font-medium text-foreground"
                    role="status"
                  >
                    Requesting Weixin login QR code...
                  </p>
                  <p className="mt-1 text-[11px] text-muted-foreground">
                    This usually takes a moment. The QR ticket and image will
                    appear here when ready.
                  </p>
                  <div className="mt-2 size-36 rounded-md border border-dashed border-border/40 bg-background/60" />
                </div>
              ) : null}

              {weixinQrPanelState === "empty" ? (
                <div className="mt-3 rounded-lg border border-border/25 bg-muted/[0.16] p-3">
                  <p className="text-[12px] font-medium text-foreground">
                    No QR code yet.
                  </p>
                  <p className="mt-1 text-[11px] text-muted-foreground">
                    Click <span className="font-medium">Get QR code</span> to
                    start Weixin login.
                  </p>
                </div>
              ) : null}

              {weixinQrPanelState === "error" ? (
                <div className="mt-3 rounded-lg border border-destructive/30 bg-destructive/[0.04] p-3">
                  <p
                    className="text-[12px] font-medium text-destructive"
                    role="status"
                    aria-live="polite"
                  >
                    Unable to get a login QR code.
                  </p>
                  <p className="mt-1 text-[11px] text-muted-foreground">
                    Check connectivity and request a new QR code.
                  </p>
                </div>
              ) : null}

              {weixinQrPanelState === "success" ? (
                <div className="mt-3 space-y-3 rounded-lg border border-border/25 bg-muted/[0.16] p-3">
                  {weixinPollingState === "success" ? (
                    <div className="rounded-lg border border-border/30 bg-muted/35 px-3 py-3">
                      <div className="flex flex-wrap items-center gap-2">
                        <Badge variant="outline" className="text-[10px]">
                          confirmed
                        </Badge>
                        <p className="text-[12px] font-medium text-foreground">
                          Weixin login confirmed and the profile is active.
                        </p>
                      </div>
                      <p className="mt-1.5 text-[11px] leading-5 text-muted-foreground">
                        The login was saved automatically. You can continue from
                        this profile without any extra save step.
                      </p>
                    </div>
                  ) : null}

                  <div className="flex flex-wrap items-start gap-3">
                    {weixinQrImageSrc ? (
                      <img
                        src={weixinQrImageSrc}
                        alt="Weixin login QR code"
                        className="size-36 rounded-md border border-border/30 bg-background object-contain"
                      />
                    ) : (
                      <div className="flex size-36 items-center justify-center rounded-md border border-dashed border-border/40 bg-background px-3 text-center">
                        <p className="text-[11px] text-muted-foreground">
                          QR image unavailable
                        </p>
                      </div>
                    )}
                    <div className="min-w-0 flex-1 space-y-1.5">
                      <p className="text-[11px] text-muted-foreground">
                        {weixinPollingState === "success"
                          ? "Login details"
                          : "QR login details"}
                      </p>
                      {weixinQrCode ? (
                        <p className="rounded-sm border border-border/30 px-2 py-1 font-mono text-[11px] break-all">
                          {weixinQrCode}
                        </p>
                      ) : null}
                      <p className="text-[11px] text-muted-foreground">
                        Polling attempts: {weixinPollingAttempts}
                      </p>
                      {!weixinQrImageSrc ? (
                        <p className="text-[11px] text-muted-foreground">
                          The server returned a login link but no directly
                          renderable image. The QR is generated locally in the
                          browser.
                        </p>
                      ) : null}
                    </div>
                  </div>
                </div>
              ) : null}

              <p
                className="mt-2.5 text-[12px] text-muted-foreground"
                role="status"
              >
                {weixinPollingStatus}
              </p>

              {weixinSaveHint ? (
                <p
                  className="mt-1.5 text-[12px] font-medium text-foreground"
                  role="status"
                  aria-live="polite"
                >
                  {weixinSaveHint}
                </p>
              ) : null}

              {weixinScanError ? (
                <p
                  className="mt-1.5 text-[12px] font-medium text-destructive"
                  role="status"
                  aria-live="polite"
                >
                  {weixinScanError}
                </p>
              ) : null}
            </section>
          ) : null}

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

            {isWeixinTransport &&
            (weixinLinkedAccountId || weixinLinkedUserId) ? (
              <div className="mb-3 grid gap-2 sm:grid-cols-2">
                {weixinLinkedAccountId ? (
                  <div className="space-y-1.5">
                    <label className="workspace-form-label">Account ID</label>
                    <p className="workspace-form-note">
                      Saved from the confirmed Weixin login session.
                    </p>
                    <Input
                      value={weixinLinkedAccountId}
                      readOnly
                      disabled
                      className="h-9 text-[13px]"
                    />
                  </div>
                ) : null}
                {weixinLinkedUserId ? (
                  <div className="space-y-1.5">
                    <label className="workspace-form-label">User ID</label>
                    <p className="workspace-form-note">
                      Bound user returned by the latest confirmed Weixin login.
                    </p>
                    <Input
                      value={weixinLinkedUserId}
                      readOnly
                      disabled
                      className="h-9 text-[13px]"
                    />
                  </div>
                ) : null}
              </div>
            ) : null}

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
