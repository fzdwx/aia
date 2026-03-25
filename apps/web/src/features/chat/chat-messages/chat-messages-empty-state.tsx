export function ChatMessagesEmptyState({ error }: { error: string | null }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-4">
      <h2 className="text-3xl font-semibold tracking-[-0.055em] text-foreground">
        What can I help with?
      </h2>
      <p className="mt-2.5 text-sm text-muted-foreground">
        Start a conversation or ask anything.
      </p>
      {error ? (
        <p className="mt-4 max-w-md text-center text-sm text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  )
}
