import { useEffect } from "react"
import { Sidebar } from "@/features/navigation/sidebar"
import { MainContent } from "@/features/navigation/main-content"
import { useChatStore } from "@/stores/chat-store"
import { connectEvents } from "@/lib/api"
import { PierreDiffProvider } from "@/features/chat/diff/pierre-diff-provider"

function App() {
  const initialize = useChatStore((s) => s.initialize)
  const handleSseEvent = useChatStore((s) => s.handleSseEvent)

  useEffect(() => {
    initialize()
    return connectEvents(handleSseEvent)
  }, [initialize, handleSseEvent])

  return (
    <PierreDiffProvider>
      <div className="flex h-screen overflow-hidden bg-background text-foreground antialiased">
        <Sidebar />
        <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <MainContent />
        </main>
      </div>
    </PierreDiffProvider>
  )
}

export default App
