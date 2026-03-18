import { useEffect, useState, type FormEvent } from "react"
import { ArrowLeft, Pencil, Plus, Trash2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"

type ChannelFormState = {
  id: string
  name: string
  enabled: boolean
  app_id: string
  app_secret: string
  base_url: string
  require_mention: boolean
  thread_mode: boolean
}

const FEISHU_BASE_URL = "https://open.feishu.cn"

function emptyChannelForm(): ChannelFormState {
  return {
    id: "",
    name: "",
    enabled: true,
    app_id: "",
    app_secret: "",
    base_url: FEISHU_BASE_URL,
    require_mention: true,
    thread_mode: true,
  }
}

export function ChannelsPanel() {
  const channelList = useChatStore((s) => s.channelList)
  const setView = useChatStore((s) => s.setView)
  const refreshChannels = useChatStore((s) => s.refreshChannels)
  const storeCreateChannel = useChatStore((s) => s.createChannel)
  const storeUpdateChannel = useChatStore((s) => s.updateChannel)
  const storeDeleteChannel = useChatStore((s) => s.deleteChannel)

  const [form, setForm] = useState<ChannelFormState>(emptyChannelForm)
  const [submitting, setSubmitting] = useState(false)
  const [editing, setEditing] = useState<string | null>(null)
  const [formOpen, setFormOpen] = useState(channelList.length === 0)

  useEffect(() => {
    refreshChannels().catch(() => {})
  }, [refreshChannels])

  useEffect(() => {
    if (channelList.length === 0 && !editing) {
      setFormOpen(true)
    }
  }, [channelList.length, editing])

  function updateForm(patch: Partial<ChannelFormState>) {
    setForm((prev) => ({ ...prev, ...patch }))
  }

  function resetForm() {
    setForm(emptyChannelForm())
    setEditing(null)
  }

  function startEdit(channelId: string) {
    const channel = channelList.find((item) => item.id === channelId)
    if (!channel) return

    setForm({
      id: channel.id,
      name: channel.name,
      enabled: channel.enabled,
      app_id: channel.app_id,
      app_secret: "",
      base_url: channel.base_url,
      require_mention: channel.require_mention,
      thread_mode: channel.thread_mode,
    })
    setEditing(channelId)
    setFormOpen(true)
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setSubmitting(true)

    try {
      if (editing) {
        await storeUpdateChannel(editing, {
          name: form.name.trim(),
          enabled: form.enabled,
          app_id: form.app_id.trim(),
          app_secret: form.app_secret,
          base_url: form.base_url.trim(),
          require_mention: form.require_mention,
          thread_mode: form.thread_mode,
        })
      } else {
        await storeCreateChannel({
          id: form.id.trim(),
          name: form.name.trim(),
          transport: "feishu",
          enabled: form.enabled,
          app_id: form.app_id.trim(),
          app_secret: form.app_secret,
          base_url: form.base_url.trim(),
          require_mention: form.require_mention,
          thread_mode: form.thread_mode,
        })
      }

      resetForm()
      setFormOpen(false)
    } finally {
      setSubmitting(false)
    }
  }

  async function handleDelete(channelId: string) {
    await storeDeleteChannel(channelId)
    if (editing === channelId) {
      resetForm()
    }
  }

  const canSubmit =
    form.id.trim() &&
    form.name.trim() &&
    form.app_id.trim() &&
    form.base_url.trim() &&
    (editing ? true : form.app_secret.trim())

  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="mx-auto max-w-[800px] px-6 py-8">
        <div className="mb-8 flex items-center gap-3">
          <button
            onClick={() => setView("chat")}
            className="flex size-8 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
          >
            <ArrowLeft className="size-4" />
          </button>
          <h1 className="text-lg font-semibold">Channels</h1>
        </div>

        <section className="mb-8">
          <h2 className="mb-3 text-[13px] font-medium text-muted-foreground">
            Configured channels
          </h2>
          {channelList.length === 0 ? (
            <p className="text-[13px] text-muted-foreground/60">
              No channels configured yet.
            </p>
          ) : (
            <div className="space-y-2">
              {channelList.map((channel) => (
                <Card
                  key={channel.id}
                  className={cn(
                    "flex items-center justify-between px-4 py-3",
                    channel.enabled && "border-foreground/20"
                  )}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-[13px] font-medium">
                        {channel.name}
                      </span>
                      <Badge variant="secondary" className="text-[10px]">
                        {channel.transport}
                      </Badge>
                      <Badge variant="outline" className="text-[10px]">
                        {channel.enabled ? "enabled" : "disabled"}
                      </Badge>
                      {channel.app_secret_set ? (
                        <Badge variant="outline" className="text-[10px]">
                          secret set
                        </Badge>
                      ) : null}
                    </div>
                    <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
                      {channel.id} · {channel.base_url}
                    </p>
                    <div className="mt-1 flex flex-wrap gap-1.5">
                      <Badge
                        variant="outline"
                        className="text-[10px] font-normal"
                      >
                        {channel.app_id}
                      </Badge>
                      <Badge
                        variant="outline"
                        className="text-[10px] font-normal"
                      >
                        {channel.require_mention
                          ? "mention required"
                          : "mention optional"}
                      </Badge>
                      <Badge
                        variant="outline"
                        className="text-[10px] font-normal"
                      >
                        {channel.thread_mode ? "thread mode" : "direct reply"}
                      </Badge>
                    </div>
                  </div>
                  <div className="ml-3 flex shrink-0 gap-1">
                    <button
                      onClick={() => startEdit(channel.id)}
                      className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground"
                    >
                      <Pencil className="size-3.5" />
                    </button>
                    <button
                      onClick={() => void handleDelete(channel.id)}
                      className="flex size-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                    >
                      <Trash2 className="size-3.5" />
                    </button>
                  </div>
                </Card>
              ))}
            </div>
          )}
        </section>

        <Separator className="mb-8 opacity-30" />

        <section>
          <div className="mb-3 flex items-center justify-between">
            <h2 className="text-[13px] font-medium text-muted-foreground">
              {editing ? `Edit Channel — ${editing}` : "Add Channel"}
            </h2>
            {!formOpen ? (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  resetForm()
                  setFormOpen(true)
                }}
              >
                <Plus className="mr-1.5 size-3.5" />
                Add
              </Button>
            ) : editing ? (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  resetForm()
                  setFormOpen(false)
                }}
              >
                Cancel
              </Button>
            ) : null}
          </div>

          {formOpen ? (
            <Card className="p-4">
              <form onSubmit={handleSubmit} className="space-y-4">
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <div>
                    <label className="mb-1 block text-[12px] text-muted-foreground">
                      ID
                    </label>
                    <Input
                      value={form.id}
                      onChange={(event) =>
                        updateForm({ id: event.target.value })
                      }
                      placeholder="e.g. feishu-main"
                      className="h-8 text-[13px]"
                      disabled={!!editing}
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-[12px] text-muted-foreground">
                      Name
                    </label>
                    <Input
                      value={form.name}
                      onChange={(event) =>
                        updateForm({ name: event.target.value })
                      }
                      placeholder="e.g. Main workspace"
                      className="h-8 text-[13px]"
                    />
                  </div>
                </div>

                <div>
                  <label className="mb-1 block text-[12px] text-muted-foreground">
                    Transport
                  </label>
                  <div className="flex h-8 items-center rounded-lg border border-input bg-transparent px-2.5 text-[13px] text-foreground">
                    Feishu
                  </div>
                </div>

                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <div>
                    <label className="mb-1 block text-[12px] text-muted-foreground">
                      App ID
                    </label>
                    <Input
                      value={form.app_id}
                      onChange={(event) =>
                        updateForm({ app_id: event.target.value })
                      }
                      placeholder="cli_xxx"
                      className="h-8 text-[13px]"
                    />
                  </div>
                  <div>
                    <label className="mb-1 block text-[12px] text-muted-foreground">
                      App Secret
                      {editing ? " (leave blank to keep existing)" : ""}
                    </label>
                    <Input
                      type="password"
                      value={form.app_secret}
                      onChange={(event) =>
                        updateForm({ app_secret: event.target.value })
                      }
                      placeholder="secret"
                      className="h-8 text-[13px]"
                    />
                  </div>
                </div>

                <div>
                  <label className="mb-1 block text-[12px] text-muted-foreground">
                    Base URL
                  </label>
                  <Input
                    value={form.base_url}
                    onChange={(event) =>
                      updateForm({ base_url: event.target.value })
                    }
                    className="h-8 text-[13px]"
                  />
                </div>

                <Separator className="opacity-30" />

                <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                  <label className="flex items-center justify-between gap-3 rounded-lg border border-border/30 bg-muted/20 px-3 py-2 text-[12px] text-foreground">
                    <span>Enabled</span>
                    <Switch
                      checked={form.enabled}
                      onCheckedChange={(checked: boolean) =>
                        updateForm({ enabled: checked })
                      }
                    />
                  </label>
                  <label className="flex items-center justify-between gap-3 rounded-lg border border-border/30 bg-muted/20 px-3 py-2 text-[12px] text-foreground">
                    <span>Require mention</span>
                    <Switch
                      checked={form.require_mention}
                      onCheckedChange={(checked: boolean) =>
                        updateForm({ require_mention: checked })
                      }
                    />
                  </label>
                  <label className="flex items-center justify-between gap-3 rounded-lg border border-border/30 bg-muted/20 px-3 py-2 text-[12px] text-foreground">
                    <span>Thread mode</span>
                    <Switch
                      checked={form.thread_mode}
                      onCheckedChange={(checked: boolean) =>
                        updateForm({ thread_mode: checked })
                      }
                    />
                  </label>
                </div>

                <Button
                  type="submit"
                  size="sm"
                  disabled={submitting || !canSubmit}
                  className="mt-2 w-full"
                >
                  <Plus className="mr-1.5 size-3.5" />
                  {editing ? "Update Channel" : "Add Channel"}
                </Button>
              </form>
            </Card>
          ) : null}
        </section>
      </div>
    </ScrollArea>
  )
}
