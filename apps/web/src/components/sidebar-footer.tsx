import { PanelRightDashed, Settings } from "lucide-react"

import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"

const SIDEBAR_FOOTER_BUTTON =
  "sidebar-nav-secondary mb-1 flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-muted-foreground transition-colors duration-150 hover:bg-muted/55 hover:text-foreground/80"

export function SidebarFooter() {
  const view = useChatStore((s) => s.view)
  const setView = useChatStore((s) => s.setView)

  return (
    <div className="p-2">
      <button
        type="button"
        onClick={() => setView(view === "trace" ? "chat" : "trace")}
        className={cn(
          SIDEBAR_FOOTER_BUTTON,
          view === "trace" && "bg-muted/65 text-foreground/82"
        )}
      >
        <PanelRightDashed className="size-[13px] opacity-35" />
        <span>Trace</span>
      </button>
      <button
        type="button"
        onClick={() => setView(view === "settings" ? "chat" : "settings")}
        className={cn(
          SIDEBAR_FOOTER_BUTTON,
          view === "settings" && "bg-muted/65 text-foreground/82"
        )}
      >
        <Settings className="size-[13px] opacity-35" />
        <span>Settings</span>
      </button>
    </div>
  )
}
