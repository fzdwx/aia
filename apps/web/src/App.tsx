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
    <div className="flex h-screen overflow-hidden bg-background text-foreground antialiased">
      <Sidebar />
      <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <MainContent />
      </main>
    </div>
  )
}

export default App
