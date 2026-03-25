import { useEffect, useMemo, useRef, useState } from "react"
import QRCode from "qrcode"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { ChannelListItem, SupportedChannelDefinition } from "@/lib/types"

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

type WeixinLoginPanelProps = {
  embedded: boolean
  definition: SupportedChannelDefinition
  configuredChannel: ChannelListItem | null
  config: Record<string, unknown>
  canWriteBotToken: boolean
  canWriteToken: boolean
  onApplyConfigPatch: (patch: Record<string, unknown>) => void
  onPersistConfig: (nextConfig: Record<string, unknown>) => Promise<void>
}

export function WeixinLoginPanel({
  embedded,
  definition,
  configuredChannel,
  config,
  canWriteBotToken,
  canWriteToken,
  onApplyConfigPatch,
  onPersistConfig,
}: WeixinLoginPanelProps) {
  const [weixinQrCode, setWeixinQrCode] = useState<string | null>(null)
  const [weixinQrImageUrl, setWeixinQrImageUrl] = useState<string | null>(null)
  const [weixinQrImageSrc, setWeixinQrImageSrc] = useState<string | null>(null)
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

  const persistedAccountId = useMemo(() => {
    return typeof config.account_id === "string" &&
      config.account_id.trim().length > 0
      ? config.account_id.trim()
      : null
  }, [config.account_id])

  const persistedUserId = useMemo(() => {
    return typeof config.user_id === "string" &&
      config.user_id.trim().length > 0
      ? config.user_id.trim()
      : null
  }, [config.user_id])

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
    const preserveWeixinScanState =
      weixinPollingState === "success" ||
      weixinPollingState === "polling" ||
      weixinPollingState === "waiting_scan" ||
      weixinPollingState === "requesting_qr" ||
      weixinPollingEnabled ||
      weixinQrCode !== null ||
      weixinQrImageUrl !== null

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
      setWeixinLinkedAccountId(persistedAccountId)
      setWeixinLinkedUserId(persistedUserId)
    }
  }, [
    configuredChannel?.id,
    definition.transport,
    persistedAccountId,
    persistedUserId,
    weixinPollingEnabled,
    weixinPollingState,
    weixinQrCode,
    weixinQrImageUrl,
  ])

  async function handleStartWeixinScanLogin() {
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
    if (!weixinPollingEnabled || !weixinQrCode) return

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
              ...config,
              ...updatePatch,
            }
            onApplyConfigPatch(updatePatch)

            try {
              await onPersistConfig(nextConfig)
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
    config,
    onApplyConfigPatch,
    onPersistConfig,
    weixinPollingEnabled,
    weixinQrCode,
  ])

  return (
    <section
      className={
        embedded
          ? "rounded-xl border border-border/30 bg-card/70 p-3 shadow-[var(--workspace-shadow)]"
          : "rounded-2xl border border-border/30 bg-card/70 p-4 shadow-[var(--workspace-shadow)]"
      }
    >
      <div className="mb-2.5">
        <div className="flex flex-wrap items-center gap-2">
          <p className="workspace-section-label text-foreground">
            Weixin QR Login
          </p>
          <Badge variant="outline" className="text-ui-xs">
            {weixinPollingState.replaceAll("_", " ")}
          </Badge>
          {weixinRawStatus ? (
            <Badge variant="outline" className="text-ui-xs">
              status: {weixinRawStatus}
            </Badge>
          ) : null}
        </div>
        <p className="workspace-panel-copy mt-1 text-muted-foreground">
          Start Weixin QR login directly from this workbench. Once the scan
          succeeds, the form is filled with the returned token and saved
          automatically.
        </p>
      </div>

      <div className="flex flex-wrap items-end gap-2">
        <Button
          type="button"
          size="lg"
          onClick={() => void handleStartWeixinScanLogin()}
          disabled={
            weixinPollingState === "requesting_qr" || weixinPollingEnabled
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
          <p className="workspace-panel-title text-foreground" role="status">
            Requesting Weixin login QR code...
          </p>
          <p className="workspace-panel-copy mt-1 text-muted-foreground">
            This usually takes a moment. The QR ticket and image will appear
            here when ready.
          </p>
          <div className="mt-2 size-36 rounded-md border border-dashed border-border/40 bg-background/60" />
        </div>
      ) : null}

      {weixinQrPanelState === "empty" ? (
        <div className="mt-3 rounded-lg border border-border/25 bg-muted/[0.16] p-3">
          <p className="workspace-panel-title text-foreground">
            No QR code yet.
          </p>
          <p className="workspace-panel-copy mt-1 text-muted-foreground">
            Click <span className="font-medium">Get QR code</span> to start
            Weixin login.
          </p>
        </div>
      ) : null}

      {weixinQrPanelState === "error" ? (
        <div className="mt-3 rounded-lg border border-destructive/30 bg-destructive/[0.04] p-3">
          <p
            className="workspace-panel-title text-destructive"
            role="status"
            aria-live="polite"
          >
            Unable to get a login QR code.
          </p>
          <p className="workspace-panel-copy mt-1 text-muted-foreground">
            Check connectivity and request a new QR code.
          </p>
        </div>
      ) : null}

      {weixinQrPanelState === "success" ? (
        <div className="mt-3 space-y-3 rounded-lg border border-border/25 bg-muted/[0.16] p-3">
          {weixinPollingState === "success" ? (
            <div className="rounded-lg border border-border/30 bg-muted/35 px-3 py-3">
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant="outline" className="text-ui-xs">
                  confirmed
                </Badge>
                <p className="workspace-panel-title text-foreground">
                  Weixin login confirmed and the profile is active.
                </p>
              </div>
              <p className="workspace-panel-copy mt-1.5 text-muted-foreground">
                The login was saved automatically. You can continue from this
                profile without any extra save step.
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
                <p className="workspace-meta text-muted-foreground">
                  QR image unavailable
                </p>
              </div>
            )}
            <div className="min-w-0 flex-1 space-y-1.5">
              <p className="workspace-meta text-muted-foreground">
                {weixinPollingState === "success"
                  ? "Login details"
                  : "QR login details"}
              </p>
              {weixinQrCode ? (
                <p className="workspace-code rounded-sm border border-border/30 px-2 py-1 break-all">
                  {weixinQrCode}
                </p>
              ) : null}
              <p className="workspace-meta text-muted-foreground">
                Polling attempts: {weixinPollingAttempts}
              </p>
              {!weixinQrImageSrc ? (
                <p className="workspace-meta text-muted-foreground">
                  The server returned a login link but no directly renderable
                  image. The QR is generated locally in the browser.
                </p>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}

      <p
        className="workspace-panel-copy mt-2.5 text-muted-foreground"
        role="status"
      >
        {weixinPollingStatus}
      </p>

      {weixinLinkedAccountId || weixinLinkedUserId ? (
        <div className="mt-3 grid gap-2 sm:grid-cols-2">
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
                className="h-8"
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
                className="h-8"
              />
            </div>
          ) : null}
        </div>
      ) : null}

      {weixinSaveHint ? (
        <p
          className="text-caption mt-1.5 font-medium text-foreground"
          role="status"
          aria-live="polite"
        >
          {weixinSaveHint}
        </p>
      ) : null}

      {weixinScanError ? (
        <p
          className="text-caption mt-1.5 font-medium text-destructive"
          role="status"
          aria-live="polite"
        >
          {weixinScanError}
        </p>
      ) : null}
    </section>
  )
}
