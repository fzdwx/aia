import { Plus, X } from "lucide-react"

import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"

const SIDEBAR_ACTION_BUTTON =
  "flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-ui-xs font-medium tracking-[0.016em] text-muted-foreground transition-colors duration-150 hover:bg-muted/55 hover:text-foreground/80"

export function SidebarSessionsView() {
  const sessions = useChatStore((s) => s.sessions)
  const activeSessionId = useChatStore((s) => s.activeSessionId)
  const sessionHydrating = useChatStore((s) => s.sessionHydrating)
  const createSession = useChatStore((s) => s.createSession)
  const switchSession = useChatStore((s) => s.switchSession)
  const deleteSession = useChatStore((s) => s.deleteSession)

  return (
    <>
      <div className="px-2 pt-2">
        <button
          type="button"
          onClick={() => createSession()}
          className={SIDEBAR_ACTION_BUTTON}
        >
          <Plus className="size-[13px] opacity-55" />
          <span>New session</span>
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-2 pt-1 pb-2">
        {sessions.map((session) => {
          const isActive = session.id === activeSessionId
          const isSwitchingTo = isActive && sessionHydrating
          const sessionLabel = session.title || session.id

          return (
            <div
              key={session.id}
              className={cn(
                "group flex w-full items-center rounded-lg px-2.5 py-1 transition-colors duration-150",
                isActive
                  ? "bg-muted text-foreground"
                  : "text-muted-foreground hover:bg-muted/45 hover:text-foreground"
              )}
            >
              <button
                type="button"
                className="min-w-0 flex-1 truncate text-left font-medium tracking-[0.012em] disabled:cursor-default"
                onClick={() => void switchSession(session.id)}
                disabled={isSwitchingTo}
                aria-current={isActive ? "page" : undefined}
                aria-busy={isSwitchingTo}
              >
                {isActive && (
                  <span className="mr-1.5 inline-block size-1.5 rounded-full bg-foreground/50" />
                )}
                <span className="text-ui">{sessionLabel}</span>
                {isSwitchingTo && (
                  <span className="text-meta ml-2 text-muted-foreground/70">
                    Loading…
                  </span>
                )}
              </button>
              <button
                type="button"
                onClick={(event) => {
                  event.stopPropagation()
                  void deleteSession(session.id)
                }}
                disabled={isSwitchingTo}
                aria-label={`Delete session ${sessionLabel}`}
                className="ml-1 hidden shrink-0 rounded p-0.5 text-muted-foreground/50 group-hover:block hover:text-foreground/80 disabled:opacity-30"
              >
                <X className="size-3" />
              </button>
            </div>
          )
        })}
      </div>
    </>
  )
}
