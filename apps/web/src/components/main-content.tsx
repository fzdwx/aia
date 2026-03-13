import { ChatMessages } from "@/components/chat-messages"
import { ChatInput } from "@/components/chat-input"
import { SettingsPanel } from "@/components/settings-panel"
import { useChatStore } from "@/stores/chat-store"

export function MainContent() {
  const view = useChatStore((s) => s.view)

  switch (view) {
    case "settings":
      return <SettingsPanel />
    default:
      return (
        <>
          <ChatMessages />
          <ChatInput />
        </>
      )
  }
}
