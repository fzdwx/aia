import { memo } from "react"

import type { StreamingTurn, TurnLifecycle } from "@/lib/types"

import { fromInvocation } from "@/features/chat/tool-timeline-helpers"
import { useChatStore } from "@/stores/chat-store"

import { MemoizedStreamingToolGroup, MemoizedToolGroup } from "./tool-timeline"
import {
  AssistantTextBlock,
  BlockRenderer,
  ThinkingBlock,
  UserMessageBlock,
} from "./message-sections/message-blocks"
import { groupBlocks, groupStreamingBlocks } from "./message-sections/grouping"
import {
  CompressionNotice,
  SessionHydratingIndicator,
  StatusIndicator,
} from "./message-sections/status-surfaces"
import { TurnMeta } from "./message-sections/turn-meta"

function withStableContentKeys(prefix: string, contents: string[]) {
  const counts = new Map<string, number>()
  return contents.map((content) => {
    const seen = counts.get(content) ?? 0
    counts.set(content, seen + 1)
    return {
      key: `${prefix}-${content}-${seen}`,
      content,
    }
  })
}

function TurnView({ turn }: { turn: TurnLifecycle }) {
  const latestRetriableTurnId = useChatStore((state) => {
    for (let index = state.turns.length - 1; index >= 0; index -= 1) {
      const candidate = state.turns[index]
      if (
        candidate &&
        (candidate.outcome === "failed" || candidate.outcome === "cancelled")
      ) {
        return candidate.turn_id
      }
    }
    return null
  })
  const canRetry =
    latestRetriableTurnId == null
      ? turn.outcome === "failed" || turn.outcome === "cancelled"
      : turn.turn_id === latestRetriableTurnId
  const grouped = groupBlocks(turn.blocks)
  const userMessages =
    turn.user_messages ?? (turn.user_message ? [turn.user_message] : [])
  const keyedUserMessages = withStableContentKeys(
    `${turn.turn_id}-user`,
    userMessages
  )

  return (
    <div
      data-turn-id={turn.turn_id}
      className="mb-8 animate-[message-in_250ms_ease-out_both] last:mb-0"
    >
      {keyedUserMessages.map(({ key, content }) => (
        <div key={key} className="mb-5">
          <UserMessageBlock content={content} />
        </div>
      ))}

      <div
        data-component="assistant-message"
        className="group/turn flex w-full flex-col gap-4 [&>*[data-type='thinking']+*[data-type='tools']]:-mt-3 [&>*[data-type='tools']+*[data-type='thinking']]:-mt-3 [&>*[data-type='tools']+*[data-type='tools']]:-mt-3"
      >
        {grouped.map((group) => {
          const groupKey =
            group.type === "tools"
              ? `${turn.turn_id}-tools-${group.invocations.map((invocation) => invocation.call.invocation_id).join("-")}`
              : group.type === "single" && group.block.kind === "thinking"
                ? `${turn.turn_id}-thinking-${group.block.content}`
                : group.type === "single" && group.block.kind === "assistant"
                  ? `${turn.turn_id}-assistant-${group.block.content}`
                  : group.type === "single" && group.block.kind === "failure"
                    ? `${turn.turn_id}-failure-${group.block.message}`
                    : `${turn.turn_id}-cancelled-${group.type === "single" && group.block.kind === "cancelled" ? group.block.message : "block"}`

          if (group.type === "tools") {
            return (
              <div key={groupKey} data-type="tools">
                <MemoizedToolGroup
                  items={group.invocations.map((invocation) =>
                    fromInvocation(invocation, turn.turn_id)
                  )}
                />
              </div>
            )
          }

          if (group.type === "single" && group.block.kind === "thinking") {
            return (
              <div key={groupKey} data-type="thinking">
                <ThinkingBlock content={group.block.content} />
              </div>
            )
          }

          return (
            <div key={groupKey} data-type="text">
              <BlockRenderer block={group.block} />
            </div>
          )
        })}
        <TurnMeta turn={turn} canRetry={canRetry} />
      </div>
    </div>
  )
}

function StreamingView({ streaming }: { streaming: StreamingTurn }) {
  const groups = groupStreamingBlocks(streaming.blocks)
  const userMessages = streaming.userMessages ?? []
  const keyedUserMessages = withStableContentKeys(
    `${streaming.turnId}-stream-user`,
    userMessages
  )
  const renderedGroups = groups.map((group, index) => {
    const previousGroup = index > 0 ? groups[index - 1] : null

    if (
      group.type === "tools" &&
      group.mergeKey === "context" &&
      previousGroup?.type === "tools" &&
      previousGroup.mergeKey === "context"
    ) {
      return {
        key: `${group.mergeKey}-${group.tools[0]?.invocationId ?? index}`,
        group,
      }
    }

    return { key: `group-${index}`, group }
  })

  return (
    <div className="mb-8 animate-[message-in_250ms_ease-out_both]">
      {keyedUserMessages.map(({ key, content }) => (
        <div key={key} className="mb-5">
          <UserMessageBlock content={content} />
        </div>
      ))}

      <div
        data-component="assistant-message"
        className="flex w-full flex-col gap-4 [&>*[data-type='thinking']+*[data-type='tools']]:-mt-3 [&>*[data-type='tools']+*[data-type='thinking']]:-mt-3 [&>*[data-type='tools']+*[data-type='tools']]:-mt-3"
      >
        {renderedGroups.map(({ key, group }, i) => {
          const isLastGroup = i === renderedGroups.length - 1

          if (group.type === "thinking") {
            const isLast = isLastGroup && streaming.status === "thinking"

            return (
              <div key={key} data-type="thinking">
                <ThinkingBlock content={group.content} isStreaming={isLast} />
              </div>
            )
          }

          if (group.type === "tools") {
            return (
              <div key={key} data-type="tools">
                <MemoizedStreamingToolGroup toolOutputs={group.tools} />
              </div>
            )
          }

          return (
            <div key={key} data-type="text">
              <AssistantTextBlock
                content={group.content}
                streaming={isLastGroup && streaming.status === "generating"}
              />
            </div>
          )
        })}
      </div>
    </div>
  )
}

export const MemoizedTurnView = memo(
  TurnView,
  (prevProps, nextProps) => prevProps.turn === nextProps.turn
)

export const MemoizedStreamingView = memo(
  StreamingView,
  (prevProps, nextProps) => prevProps.streaming === nextProps.streaming
)

export {
  CompressionNotice,
  SessionHydratingIndicator,
  StatusIndicator,
  UserMessageBlock,
}
