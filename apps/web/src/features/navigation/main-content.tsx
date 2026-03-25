import { Suspense, lazy, type ReactNode } from "react"

import { ChatMessages } from "@/features/chat/chat-messages"
import { ChatInput } from "@/features/chat/chat-input"
import { useChatStore } from "@/stores/chat-store"

const SettingsPanel = lazy(async () => {
  const module = await import("@/features/settings")
  return { default: module.SettingsPanel }
})

const TracePanel = lazy(async () => {
  const module = await import("@/features/trace")
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
      return renderSecondaryPanel(<SettingsPanel />)
    default:
      return (
        <>
          <ChatMessages />
          <ChatInput />
        </>
      )
  }
}
