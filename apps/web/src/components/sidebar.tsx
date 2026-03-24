import { useEffect } from "react"
import { PanelRightDashed, Plus, Settings, X } from "lucide-react"
import { Separator } from "@/components/ui/separator"
import { TraceSidebar } from "@/components/trace-sidebar"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"
import { useChannelsStore } from "@/stores/channels-store"

const SIDEBAR_SECTION_LABEL = "workspace-section-label text-muted-foreground/70"
const SIDEBAR_BUTTON_TEXT = "text-ui-sm font-medium tracking-[0.02em]"
const SIDEBAR_NAV_BUTTON_TEXT = "text-ui-xs font-medium tracking-[0.016em]"
const SIDEBAR_SESSION_TEXT = "font-medium tracking-[0.012em]"
const SIDEBAR_SESSION_TITLE = "text-ui-sm"

export function Sidebar() {
  const view = useChatStore((s) => s.view)
  const setView = useChatStore((s) => s.setView)
  const settingsSection = useChatStore((s) => s.settingsSection)
  const setSettingsSection = useChatStore((s) => s.setSettingsSection)
  const sessions = useChatStore((s) => s.sessions)
  const activeSessionId = useChatStore((s) => s.activeSessionId)
  const sessionHydrating = useChatStore((s) => s.sessionHydrating)
  const createSession = useChatStore((s) => s.createSession)
  const switchSession = useChatStore((s) => s.switchSession)
  const deleteSession = useChatStore((s) => s.deleteSession)
  const initializeChannels = useChannelsStore((s) => s.initialize)

  useEffect(() => {
    if (view !== "settings") return
    void initializeChannels().catch(() => {})
  }, [initializeChannels, view])

  return (
    <aside className="flex h-full w-[280px] shrink-0 flex-col border-r border-border/50 bg-sidebar">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3">
        <span className="workspace-brand">aia</span>
      </div>

      <Separator className="opacity-30" />

      {view === "trace" ? (
        <TraceSidebar />
      ) : view === "settings" ? (
        <div className="flex-1 overflow-y-auto px-2 py-2">
          <div className="px-2.5 pb-2">
            <p className={SIDEBAR_SECTION_LABEL}>Settings</p>
          </div>

          <div className="space-y-1">
            <button
              type="button"
              onClick={() => setSettingsSection("providers")}
              className={cn(
                `flex w-full items-center gap-2.5 rounded-lg px-2.5 py-[7px] transition-colors duration-150 ${SIDEBAR_BUTTON_TEXT}`,
                settingsSection === "providers"
                  ? "bg-muted/65 text-foreground/82"
                  : "text-muted-foreground hover:bg-muted/45 hover:text-foreground/80"
              )}
            >
              <Settings className="size-[14px] opacity-40" />
              <span>Providers</span>
            </button>
            <button
              type="button"
              onClick={() => setSettingsSection("channels")}
              className={cn(
                `flex w-full items-center gap-2.5 rounded-lg px-2.5 py-[7px] transition-colors duration-150 ${SIDEBAR_BUTTON_TEXT}`,
                settingsSection === "channels"
                  ? "bg-muted/65 text-foreground/82"
                  : "text-muted-foreground hover:bg-muted/45 hover:text-foreground/80"
              )}
            >
              <PanelRightDashed className="size-[14px] opacity-40" />
              <span>Channels</span>
            </button>
          </div>
        </div>
      ) : (
        <>
          <div className="px-2 pt-2">
            <button
              onClick={() => createSession()}
              className={`flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-muted-foreground transition-colors duration-150 hover:bg-muted/55 hover:text-foreground/80 ${SIDEBAR_NAV_BUTTON_TEXT}`}
            >
              <Plus className="size-[13px] opacity-55" />
              <span>New session</span>
            </button>
          </div>

          <div className="flex-1 overflow-y-auto px-2 pt-1 pb-2">
            {sessions.map((session) => {
              const isActive = session.id === activeSessionId
              const isSwitchingTo = isActive && sessionHydrating

              return (
                <div
                  key={session.id}
                  className={cn(
                    "group flex w-full items-center rounded-lg px-2.5 py-2 transition-colors duration-150",
                    isActive
                      ? "bg-muted/65 text-foreground/82"
                      : "text-muted-foreground hover:bg-muted/45 hover:text-foreground/80"
                  )}
                >
                  <button
                    className={cn(
                      "min-w-0 flex-1 truncate text-left disabled:cursor-default",
                      SIDEBAR_SESSION_TEXT
                    )}
                    onClick={() => void switchSession(session.id)}
                    disabled={isSwitchingTo}
                    aria-current={isActive ? "page" : undefined}
                    aria-busy={isSwitchingTo}
                  >
                    {isActive && (
                      <span className="mr-1.5 inline-block size-1.5 rounded-full bg-foreground/50" />
                    )}
                    <span className={SIDEBAR_SESSION_TITLE}>
                      {session.title || session.id}
                    </span>
                    {isSwitchingTo && (
                      <span className="text-meta ml-2 text-muted-foreground/70">
                        Loading…
                      </span>
                    )}
                  </button>
                  <button
                    onClick={(e) => {
                      e.stopPropagation()
                      void deleteSession(session.id)
                    }}
                    disabled={isSwitchingTo}
                    className="ml-1 hidden shrink-0 rounded p-0.5 text-muted-foreground/50 group-hover:block hover:text-foreground/80 disabled:opacity-30"
                  >
                    <X className="size-3" />
                  </button>
                </div>
              )
            })}
          </div>
        </>
      )}

      <Separator className="opacity-30" />

      {/* Footer */}
      <div className="p-2">
        <button
          onClick={() => setView(view === "trace" ? "chat" : "trace")}
          className={cn(
            `mb-1 flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-muted-foreground transition-colors duration-150 hover:bg-muted/55 hover:text-foreground/80 ${SIDEBAR_NAV_BUTTON_TEXT}`,
            view === "trace" && "bg-muted/65 text-foreground/82"
          )}
        >
          <PanelRightDashed className="size-[13px] opacity-35" />
          <span>Trace</span>
        </button>
        <button
          onClick={() => setView(view === "settings" ? "chat" : "settings")}
          className={cn(
            `mb-1 flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-muted-foreground transition-colors duration-150 hover:bg-muted/55 hover:text-foreground/80 ${SIDEBAR_NAV_BUTTON_TEXT}`,
            view === "settings" && "bg-muted/65 text-foreground/82"
          )}
        >
          <Settings className="size-[13px] opacity-35" />
          <span>Settings</span>
        </button>
      </div>
    </aside>
  )
}
