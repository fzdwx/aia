import { Plus, X } from "lucide-react"

import { useChatStore } from "@/stores/chat-store"

const SIDEBAR_ACTION_BUTTON =
  "sidebar-nav-secondary flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-muted-foreground transition-colors duration-150 hover:bg-muted/55 hover:text-foreground/80"

const SIDEBAR_SESSION_ITEM =
  "sidebar-session-item group flex w-full items-center gap-1 rounded-lg"

function formatLastActiveAt(timestamp: string) {
  if (!timestamp) return ""

  const date = new Date(timestamp)
  if (Number.isNaN(date.getTime())) return ""

  const diffMs = Date.now() - date.getTime()
  const diffMinutes = Math.floor(diffMs / 60000)
  if (diffMinutes <= 0) return "now"
  if (diffMinutes < 60) return `${diffMinutes}m`

  const diffHours = Math.floor(diffMinutes / 60)
  if (diffHours < 24) return `${diffHours}h`

  const diffDays = Math.floor(diffHours / 24)
  if (diffDays < 7) return `${diffDays}d`

  return date.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  })
}

export function SidebarSessions() {
  const sessions = useChatStore((s) => s.sessions)
  const sessionTitleAnimations = useChatStore((s) => s.sessionTitleAnimations)
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

      <div className="flex-1 overflow-y-auto px-2 pt-1.5 pb-2">
        <div className="space-y-1">
          {sessions.map((session) => {
            const isActive = session.id === activeSessionId
            const isSwitchingTo = isActive && sessionHydrating
            const animation = sessionTitleAnimations[session.id]
            const sessionLabel =
              (animation?.animating
                ? animation.renderedTitle
                : session.title) || session.id
            const lastActiveLabel = formatLastActiveAt(session.last_active_at)

            return (
              <div
                key={session.id}
                data-selected={isActive ? "true" : "false"}
                className={SIDEBAR_SESSION_ITEM}
              >
                <button
                  type="button"
                  className="sidebar-nav-primary flex min-w-0 flex-1 items-center gap-1.5 px-2.5 py-2 text-left disabled:cursor-default"
                  onClick={() => void switchSession(session.id)}
                  disabled={isSwitchingTo}
                  aria-current={isActive ? "page" : undefined}
                  aria-busy={isSwitchingTo}
                >
                  {isActive ? (
                    <span className="inline-block size-1.5 shrink-0 rounded-full bg-foreground/60" />
                  ) : null}
                  <span className="min-w-0 flex-1 truncate">{sessionLabel}</span>
                  {isSwitchingTo ? (
                    <span className="text-meta shrink-0 text-muted-foreground/72">
                      Loading…
                    </span>
                  ) : !isActive && lastActiveLabel ? (
                    <span className="text-meta shrink-0 text-muted-foreground/72">
                      {lastActiveLabel}
                    </span>
                  ) : null}
                </button>
                {isActive ? (
                  <button
                    type="button"
                    onClick={(event) => {
                      event.stopPropagation()
                      void deleteSession(session.id)
                    }}
                    disabled={isSwitchingTo}
                    aria-label={`Delete session ${sessionLabel}`}
                    className="sidebar-session-delete mr-1 shrink-0 disabled:opacity-30"
                  >
                    <X className="size-3" />
                  </button>
                ) : null}
              </div>
            )
          })}
        </div>
      </div>
    </>
  )
}
