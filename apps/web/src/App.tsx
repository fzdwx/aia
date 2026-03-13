import { useEffect } from "react"
import { Sidebar } from "@/components/sidebar"
import { MainContent } from "@/components/main-content"
import { useChatStore } from "@/stores/chat-store"
import { connectEvents } from "@/lib/api"

function App() {
  const initialize = useChatStore((s) => s.initialize)
  const handleSseEvent = useChatStore((s) => s.handleSseEvent)

  useEffect(() => {
    initialize()
    return connectEvents(handleSseEvent)
  }, [initialize, handleSseEvent])

  return (
    <div className="flex h-screen bg-background text-foreground antialiased">
      <Sidebar />
      <main className="flex min-w-0 flex-1 flex-col">
        <MainContent />
      </main>
    </div>
  )
}

export default App
