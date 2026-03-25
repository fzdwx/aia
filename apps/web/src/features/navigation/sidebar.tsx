import { useEffect } from "react"
import { Separator } from "@/components/ui/separator"
import { SidebarFooter } from "./sidebar-footer"
import { SidebarSettings } from "./sidebar-settings-view"
import { SidebarSessions } from "./sidebar-sessions-view"
import { TraceSidebar } from "@/features/trace/sidebar"
import { useChannelsStore } from "@/stores/channels-store"
import { useWorkbenchStore } from "@/stores/workbench-store"

export function Sidebar() {
  const view = useWorkbenchStore((s) => s.view)
  const initializeChannels = useChannelsStore((s) => s.initialize)

  useEffect(() => {
    if (view !== "settings") return
    void initializeChannels().catch(() => {})
  }, [initializeChannels, view])

  return (
    <aside className="flex h-full w-[280px] shrink-0 flex-col border-r border-border/50 bg-sidebar">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3">
        <span className="workspace-brand sidebar-brand-mark">aia</span>
      </div>

      <Separator className="opacity-30" />

      {view === "trace" ? (
        <TraceSidebar />
      ) : view === "settings" ? (
        <SidebarSettings />
      ) : (
        <SidebarSessions />
      )}

      <Separator className="opacity-30" />

      <SidebarFooter />
    </aside>
  )
}
