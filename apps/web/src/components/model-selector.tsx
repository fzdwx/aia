import { Check } from "lucide-react"

import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { cn } from "@/lib/utils"
import { useChatStore } from "@/stores/chat-store"
import { useSessionSettingsStore } from "@/stores/session-settings-store"

export function ModelSelector() {
  const providerList = useChatStore((s) => s.providerList)
  const refreshProviders = useChatStore((s) => s.refreshProviders)
  const chatState = useChatStore((s) => s.chatState)
  const sessionSettings = useSessionSettingsStore((s) => s.sessionSettings)
  const hydrating = useSessionSettingsStore((s) => s.hydrating)
  const updating = useSessionSettingsStore((s) => s.updating)
  const switchModel = useSessionSettingsStore((s) => s.switchModel)

  const activeProviderName = sessionSettings?.provider
  const activeModelId = sessionSettings?.model
  const activeProvider = providerList.find((p) => p.name === activeProviderName)
  const activeModel = activeProvider?.models.find((m) => m.id === activeModelId)
  const displayLabel = hydrating
    ? "loading model..."
    : (activeModel?.display_name ?? activeModelId ?? "no model")

  if (providerList.length === 0) return null

  return (
    <div className="min-w-0">
      <Select
        disabled={hydrating || updating || chatState === "active"}
        value={
          activeProviderName && activeModelId
            ? `${activeProviderName}::${activeModelId}`
            : undefined
        }
        onValueChange={(value) => {
          if (!value) return
          const [providerName, modelId] = value.split("::")
          if (!providerName || !modelId) return
          void switchModel(providerList, providerName, modelId)
            .then(() => refreshProviders())
            .catch(() => {})
        }}
      >
        <SelectTrigger
          size="sm"
          className="h-7 max-w-[220px] border-0 bg-transparent px-1.5 py-0 text-[11px] text-muted-foreground shadow-none hover:bg-accent/50 hover:text-foreground/80 disabled:opacity-50"
        >
          <SelectValue>{displayLabel}</SelectValue>
        </SelectTrigger>
        <SelectContent
          align="start"
          alignItemWithTrigger={false}
          className="min-w-[220px]"
        >
          {providerList.map((provider) => (
            <SelectGroup key={provider.name}>
              <SelectLabel className="px-2.5 pt-2 pb-1 text-[10px] font-semibold tracking-wider text-muted-foreground/50 uppercase">
                {provider.name}
              </SelectLabel>
              {provider.models.map((model) => {
                const isActive =
                  provider.name === activeProviderName &&
                  model.id === activeModelId
                return (
                  <SelectItem
                    key={`${provider.name}-${model.id}`}
                    value={`${provider.name}::${model.id}`}
                    className={cn(
                      "px-2.5 py-1.5 text-[12px]",
                      !isActive && "text-muted-foreground"
                    )}
                  >
                    <span className="flex w-full items-center justify-between font-medium">
                      <span>{model.display_name ?? model.id}</span>
                      {isActive && <Check className="ml-2 size-3 shrink-0" />}
                    </span>
                  </SelectItem>
                )
              })}
            </SelectGroup>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
