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
import { useProviderRegistryStore } from "@/stores/provider-registry-store"
import { switchActiveSessionModel } from "@/stores/session-settings-runtime"
import { useSessionSettingsStore } from "@/stores/session-settings-store"

const MODEL_SELECTOR_LABEL =
  "workspace-section-label px-2.5 pt-2 pb-1 text-muted-foreground/50"
const MODEL_SELECTOR_ITEM = "text-ui px-2.5 py-1.5"

export function ModelSelector() {
  const providerList = useProviderRegistryStore((s) => s.providerList)
  const chatState = useChatStore((s) => s.chatState)
  const sessionSettings = useSessionSettingsStore((s) => s.sessionSettings)
  const hydrating = useSessionSettingsStore((s) => s.hydrating)
  const updating = useSessionSettingsStore((s) => s.updating)

  const activeProviderName = sessionSettings?.provider
  const activeModelId = sessionSettings?.model
  const selectedValue =
    activeProviderName && activeModelId
      ? `${activeProviderName}::${activeModelId}`
      : ""
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
        value={selectedValue}
        onValueChange={(value) => {
          if (!value) return
          const [providerName, modelId] = value.split("::")
          if (!providerName || !modelId) return
          void switchActiveSessionModel(providerName, modelId).catch(() => {})
        }}
      >
        <SelectTrigger
          size="sm"
          className="text-ui h-8 max-w-[240px] border-0 bg-transparent px-2 py-1 font-medium text-muted-foreground/90 shadow-none hover:bg-muted/60 hover:text-foreground/80 disabled:opacity-50 [&_svg:not([class*='size-'])]:size-3"
        >
          <SelectValue className="text-ui">{displayLabel}</SelectValue>
        </SelectTrigger>
        <SelectContent
          align="start"
          alignItemWithTrigger={false}
          className="min-w-[220px]"
        >
          {providerList.map((provider) => (
            <SelectGroup key={provider.name}>
              <SelectLabel className={MODEL_SELECTOR_LABEL}>
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
                      MODEL_SELECTOR_ITEM,
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
