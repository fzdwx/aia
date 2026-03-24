import { useEffect } from "react"
import { Separator } from "@/components/ui/separator"
import { SidebarFooter } from "@/components/sidebar-footer"
import { SidebarSettingsView } from "@/components/sidebar-settings-view"
import { SidebarSessionsView } from "@/components/sidebar-sessions-view"
import { TraceSidebar } from "@/components/trace-sidebar"
import { useChatStore } from "@/stores/chat-store"
import { useChannelsStore } from "@/stores/channels-store"

export function Sidebar() {
  const view = useChatStore((s) => s.view)
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
        <SidebarSettingsView />
      ) : (
        <SidebarSessionsView />
      )}

      <Separator className="opacity-30" />

      <SidebarFooter />
    </aside>
  )
}
