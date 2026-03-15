import { PanelRightDashed, Plus, Settings, X } from "lucide-react"
import { Separator } from "@/components/ui/separator"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"

export function Sidebar() {
  const provider = useChatStore((s) => s.provider)
  const view = useChatStore((s) => s.view)
  const setView = useChatStore((s) => s.setView)
  const sessions = useChatStore((s) => s.sessions)
  const activeSessionId = useChatStore((s) => s.activeSessionId)
  const createSession = useChatStore((s) => s.createSession)
  const switchSession = useChatStore((s) => s.switchSession)
  const deleteSession = useChatStore((s) => s.deleteSession)

  return (
    <aside className="flex h-full w-[280px] shrink-0 flex-col border-r border-border/50 bg-sidebar">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3">
        <span className="font-serif text-[17px] font-semibold tracking-tight">
          aia
        </span>
      </div>

      <Separator className="opacity-30" />

      {/* New session button */}
      <div className="px-2 pt-2">
        <button
          onClick={() => createSession()}
          className="flex w-full items-center gap-2 rounded-lg px-2.5 py-[7px] text-[13px] text-muted-foreground transition-colors duration-150 hover:bg-accent/50 hover:text-foreground/80"
        >
          <Plus className="size-[14px] opacity-60" />
          <span>New session</span>
        </button>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto px-2 pt-1 pb-2">
        {sessions.map((session) => (
          <div
            key={session.id}
            className={cn(
              "group flex w-full items-center rounded-lg px-2.5 py-[7px] text-[13px] transition-colors duration-150",
              session.id === activeSessionId
                ? "bg-accent/50 text-foreground/80"
                : "text-muted-foreground hover:bg-accent/30 hover:text-foreground/80"
            )}
          >
            <button
              className="min-w-0 flex-1 truncate text-left"
              onClick={() => switchSession(session.id)}
            >
              {session.id === activeSessionId && (
                <span className="mr-1.5 inline-block size-1.5 rounded-full bg-foreground/50" />
              )}
              {session.title || session.id}
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation()
                deleteSession(session.id)
              }}
              className="ml-1 hidden shrink-0 rounded p-0.5 text-muted-foreground/50 hover:text-foreground/80 group-hover:block"
            >
              <X className="size-3" />
            </button>
          </div>
        ))}
      </div>

      <Separator className="opacity-30" />

      {/* Provider info */}
      {provider && (
        <div className="px-4 py-3">
          <div className="rounded-lg border border-border/30 bg-muted/30 px-3 py-2.5">
            <div className="flex items-center gap-2">
              <span
                className={cn(
                  "size-1.5 rounded-full",
                  provider.connected ? "bg-green-500" : "bg-destructive"
                )}
              />
              <span className="text-[12px] font-medium text-foreground/80">
                {provider.name}
              </span>
            </div>
            <p className="mt-1 pl-3.5 text-[11px] text-muted-foreground">
              {provider.model}
            </p>
          </div>
        </div>
      )}

      <Separator className="opacity-30" />

      {/* Footer */}
      <div className="p-2">
        <button
          onClick={() => setView(view === "trace" ? "chat" : "trace")}
          className={cn(
            "mb-1 flex w-full items-center gap-2.5 rounded-lg px-2.5 py-[7px] text-[13px] text-muted-foreground transition-colors duration-150 hover:bg-accent/50 hover:text-foreground/80",
            view === "trace" && "bg-accent/50 text-foreground/80"
          )}
        >
          <PanelRightDashed className="size-[14px] opacity-40" />
          <span>Trace</span>
        </button>
        <button
          onClick={() => setView(view === "settings" ? "chat" : "settings")}
          className={cn(
            "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-[7px] text-[13px] text-muted-foreground transition-colors duration-150 hover:bg-accent/50 hover:text-foreground/80",
            view === "settings" && "bg-accent/50 text-foreground/80"
          )}
        >
          <Settings className="size-[14px] opacity-40" />
          <span>Settings</span>
        </button>
      </div>
    </aside>
  )
}
