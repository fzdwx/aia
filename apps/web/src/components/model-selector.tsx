import { useEffect, useRef, useState } from "react"
import { ChevronDown, Check } from "lucide-react"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"

export function ModelSelector() {
  const providerList = useChatStore((s) => s.providerList)
  const sessionSettings = useChatStore((s) => s.sessionSettings)
  const switchModel = useChatStore((s) => s.switchModel)
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  const activeProviderName = sessionSettings?.provider
  const activeModelId = sessionSettings?.model
  const activeProvider = providerList.find((p) => p.name === activeProviderName)

  const activeModel = activeProvider?.models.find((m) => m.id === activeModelId)
  const displayLabel = activeModel?.display_name ?? activeModelId ?? "no model"

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    if (open) {
      document.addEventListener("mousedown", handleClickOutside)
    }
    return () => document.removeEventListener("mousedown", handleClickOutside)
  }, [open])

  if (providerList.length === 0) return null

  return (
    <div ref={ref} className="relative mb-1.5">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] text-muted-foreground transition-colors hover:bg-accent/50 hover:text-foreground/80"
      >
        <span>{displayLabel}</span>
        <ChevronDown
          className={cn("size-3 transition-transform", open && "rotate-180")}
        />
      </button>

      {open && (
        <div className="absolute bottom-full left-0 z-50 mb-1 min-w-[220px] rounded-lg border border-border/50 bg-popover p-1 shadow-lg">
          {providerList.map((p) => (
            <div key={p.name}>
              <div className="px-2.5 pt-2 pb-1 text-[10px] font-semibold tracking-wider text-muted-foreground/50 uppercase">
                {p.name}
              </div>
              {p.models.map((m) => {
                const isActive = p.name === activeProviderName && m.id === activeModelId
                const reasoningEffort =
                  sessionSettings?.provider === p.name && sessionSettings?.model === m.id
                    ? sessionSettings.reasoning_effort
                    : m.reasoning_effort
                return (
                  <button
                    key={`${p.name}-${m.id}`}
                    type="button"
                    onClick={() => {
                      switchModel(p.name, m.id, reasoningEffort ?? null)
                      setOpen(false)
                    }}
                    className={cn(
                      "flex w-full items-center justify-between rounded-md px-2.5 py-1.5 text-left text-[12px] transition-colors hover:bg-accent/50",
                      isActive && "text-foreground",
                      !isActive && "text-muted-foreground"
                    )}
                  >
                    <span className="font-medium">
                      {m.display_name ?? m.id}
                    </span>
                    {isActive && <Check className="ml-2 size-3 shrink-0" />}
                  </button>
                )
              })}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
