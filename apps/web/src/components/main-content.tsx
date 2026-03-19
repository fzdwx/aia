import { Suspense, lazy, type ReactNode } from "react"

import { ChatMessages } from "@/components/chat-messages"
import { ChatInput } from "@/components/chat-input"
import { useChatStore } from "@/stores/chat-store"

const ChannelsPanel = lazy(async () => {
  const module = await import("@/components/channels-panel")
  return { default: module.ChannelsPanel }
})

const SettingsPanel = lazy(async () => {
  const module = await import("@/components/settings-panel")
  return { default: module.SettingsPanel }
})

const TracePanel = lazy(async () => {
  const module = await import("@/components/trace-panel")
  return { default: module.TracePanel }
})

function SecondaryPanelFallback() {
  return (
    <div className="flex h-full min-h-0 items-center justify-center px-6 py-10 text-sm text-muted-foreground">
      正在加载面板…
    </div>
  )
}

function renderSecondaryPanel(panel: ReactNode) {
  return <Suspense fallback={<SecondaryPanelFallback />}>{panel}</Suspense>
}

export function MainContent() {
  const view = useChatStore((s) => s.view)

  switch (view) {
    case "settings":
      return renderSecondaryPanel(<SettingsPanel />)
    case "trace":
      return renderSecondaryPanel(<TracePanel />)
    case "channels":
      return renderSecondaryPanel(<ChannelsPanel />)
    default:
      return (
        <>
          <ChatMessages />
          <ChatInput />
        </>
      )
  }
}
