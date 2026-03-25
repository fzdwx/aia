import { PanelRightDashed, Settings } from "lucide-react"

import { cn } from "@/lib/utils"
import {
  type SettingsSection,
  useWorkbenchStore,
} from "@/stores/workbench-store"

const SETTINGS_NAV_ITEMS: Array<{
  icon: typeof Settings
  id: SettingsSection
  label: string
}> = [
  {
    icon: Settings,
    id: "providers",
    label: "Providers",
  },
  {
    icon: PanelRightDashed,
    id: "channels",
    label: "Channels",
  },
]

export function SidebarSettings() {
  const settingsSection = useWorkbenchStore((s) => s.settingsSection)
  const setSettingsSection = useWorkbenchStore((s) => s.setSettingsSection)

  return (
    <div className="flex-1 overflow-y-auto px-2 py-2">
      <div className="px-2.5 pb-2">
        <p className="workspace-section-label text-muted-foreground/70">
          Settings
        </p>
      </div>

      <div className="space-y-1">
        {SETTINGS_NAV_ITEMS.map(({ icon: Icon, id, label }) => {
          const isActive = settingsSection === id

          return (
            <button
              key={id}
              type="button"
              onClick={() => setSettingsSection(id)}
              aria-current={isActive ? "page" : undefined}
              className={cn(
                "sidebar-nav-primary flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 transition-colors duration-150",
                isActive
                  ? "bg-muted/65 text-foreground/82"
                  : "text-muted-foreground hover:bg-muted/45 hover:text-foreground/80"
              )}
            >
              <Icon className="size-[14px] opacity-40" />
              <span>{label}</span>
            </button>
          )
        })}
      </div>
    </div>
  )
}
