import { memo } from "react"

import type { StreamingTurn, TurnLifecycle } from "@/lib/types"

import { fromInvocation } from "@/features/chat/tool-timeline-helpers"

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

function TurnView({ turn }: { turn: TurnLifecycle }) {
  const grouped = groupBlocks(turn.blocks)
  const userMessages = turn.user_messages ?? (turn.user_message ? [turn.user_message] : [])

  return (
      <div
          data-turn-id={turn.turn_id}
          className="mb-8 animate-[message-in_250ms_ease-out_both] last:mb-0"
      >
        {userMessages.map((content, i) => (
          <div key={i} className="mb-5">
            <UserMessageBlock content={content} />
          </div>
        ))}

        <div
            data-component="assistant-message"
            className="group/turn flex w-full flex-col [&>*+*]:mt-4 [&>*[data-type='tools']+*[data-type='tools']]:mt-1.5"
        >
          {grouped.map((group, i) => {
            if (group.type === "tools") {
              return (
                  <div key={i} data-type="tools">
                    <MemoizedToolGroup
                        items={group.invocations.map(fromInvocation)}
                    />
                  </div>
              )
            }

            return (
                <div key={i} data-type="text">
                  <BlockRenderer block={group.block} />
                </div>
            )
          })}
          <TurnMeta turn={turn} />
        </div>
      </div>
  )
}

function StreamingView({ streaming }: { streaming: StreamingTurn }) {
  const groups = groupStreamingBlocks(streaming.blocks)

  return (
      <div className="mb-8 animate-[message-in_250ms_ease-out_both]">
        {streaming.userMessage ? (
            <div className="mb-5">
              <UserMessageBlock content={streaming.userMessage} />
            </div>
        ) : null}

        <div
            data-component="assistant-message"
            className="flex w-full flex-col [&>*+*]:mt-4 [&>*[data-type='tools']+*[data-type='tools']]:mt-1.5"
        >
          {groups.map((group, i) => {
            if (group.type === "thinking") {
              const isLast =
                  i === groups.length - 1 && streaming.status === "thinking"

              return (
                  <div key={i} data-type="text">
                    <ThinkingBlock
                        content={group.content}
                        isStreaming={isLast}
                    />
                  </div>
              )
            }

            if (group.type === "tools") {
              return (
                  <div key={i} data-type="tools">
                    <MemoizedStreamingToolGroup
                        toolOutputs={group.tools}
                        keepContextGroupsOpen
                    />
                  </div>
              )
            }

            return (
                <div key={i} data-type="text">
                  <AssistantTextBlock content={group.content} streaming />
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
