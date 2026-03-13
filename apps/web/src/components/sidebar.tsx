import { Settings } from "lucide-react"
import { Separator } from "@/components/ui/separator"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"

export function Sidebar() {
  const provider = useChatStore((s) => s.provider)
  const view = useChatStore((s) => s.view)
  const setView = useChatStore((s) => s.setView)

  return (
    <aside className="flex h-full w-[280px] shrink-0 flex-col border-r border-border/50 bg-sidebar">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3">
        <span className="font-serif text-[17px] font-semibold tracking-tight">
          aia
        </span>
      </div>

      <Separator className="opacity-30" />

      {/* Provider info */}
      <div className="flex-1 px-4 pt-4">
        {provider && (
          <div className="rounded-lg border border-border/30 bg-muted/30 px-3 py-2.5">
            <div className="flex items-center gap-2">
              <span
                className={cn(
                  "size-1.5 rounded-full",
                  provider.connected ? "bg-green-500" : "bg-destructive",
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
        )}
      </div>

      <Separator className="opacity-30" />

      {/* Footer */}
      <div className="p-2">
        <button
          onClick={() => setView(view === "settings" ? "chat" : "settings")}
          className={cn(
            "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-[7px] text-[13px] text-muted-foreground transition-colors duration-150 hover:bg-accent/50 hover:text-foreground/80",
            view === "settings" && "bg-accent/50 text-foreground/80",
          )}
        >
          <Settings className="size-[14px] opacity-40" />
          <span>Settings</span>
        </button>
      </div>
    </aside>
  )
}
