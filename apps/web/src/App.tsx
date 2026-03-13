import { Sidebar } from "@/components/sidebar"
import { ChatMessages } from "@/components/chat-messages"
import { ChatInput } from "@/components/chat-input"
import { useChat } from "@/hooks/use-chat"

function App() {
  const { turns, streamingTurn, chatState, provider, error, submitTurn } =
    useChat()

  return (
    <div className="flex h-screen bg-background text-foreground antialiased">
      <Sidebar provider={provider} />
      <main className="flex min-w-0 flex-1 flex-col">
        <ChatMessages
          turns={turns}
          streamingTurn={streamingTurn}
          error={error}
        />
        <ChatInput onSend={submitTurn} disabled={chatState === "active"} />
      </main>
    </div>
  )
}

export default App
