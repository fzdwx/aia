---
rfc: 0004
name: message-queue-mechanism
title: Message Queue and Interrupt Mechanism
description: Introduces message queue for queuing messages during agent execution, with optional ESC-based immediate interrupt.
status: Implemented
date: 2026-03-29
authors:
  - aia
supersedes: []
superseded_by: null
---

# RFC 0004: Message Queue and Interrupt Mechanism

## Summary

为 `aia` 引入消息队列机制：
1. **消息入队** - 用户在 agent 执行时发送消息，等待当前 tool/chat 完成后入队
2. **ESC 打断** - 用户按 ESC 可立即打断当前 turn
3. **自动处理** - turn 结束后自动处理队列中的消息

## Implementation Snapshot

截至当前代码，RFC 头部的 `Implemented` 结论成立。当前真实实现重点是：

1. Web 主发送路径已经切到 `POST /api/session/message`，由 server 决定“立即开始 / 入队等待”
2. 队列相关控制面与 SSE 事件已存在：`message_queued`、`message_deleted`、`turn_interrupted`、`queue_processing`
3. session 内存态包含 `message_queue`、`interrupt_requested`、`queue_processing` 等字段
4. queue 中的消息会在 turn 返回后继续处理；删除与 dequeue 事实会写入 tape
5. “等待 question 时不接受普通 turn”这条约束仍优先于消息队列
6. 当前 `handle_queue_message(...)` 在 session 处于 `Running` 时不会立即把 `message_queued` 追加进 tape，而是只更新内存队列并广播 SSE，以避免和正在持有 runtime/tape 的 TurnWorker 发生并发写入冲突；因此如果 server 在当前 turn 结束前重启，这段时间里临时入队的消息仍可能丢失。`restore_queue_from_tape(...)` 只能恢复那些已经存在于 tape 中的 queue 事件

阅读下文时要注意：正文里有不少大段伪代码和中间命名，是 RFC 起草时的说明稿，不等于逐行对应当前代码。当前实现应优先对照：

- `apps/agent-server/src/session_manager/{mod.rs,message_queue.rs,types.rs,handle.rs,turn_execution.rs}`
- `apps/agent-server/src/routes/session/{mod.rs,handlers.rs}`
- `apps/agent-server/src/sse/mod.rs`

## Motivation

当前用户在 agent 执行 turn 时发送消息：
- 消息被拒绝（session busy）
- 或用户必须等待当前 turn 完成

用户需要更灵活的交互方式：
- 想在当前操作完成后追加指令
- 想在紧急情况下立即打断 agent

## Goals

1. 允许用户在 agent 执行时发送消息，排队等待处理
2. 允许用户按 ESC 立即打断当前 turn
3. 保持 append-only session tape 语义
4. 提供清晰的 API 端点和 SSE 事件

## Non-Goals

1. 不做复杂的消息优先级系统
2. 不实现多人协作场景的消息仲裁
3. 不自动合并或优化连续消息

## Existing Code Analysis

当前代码库已有以下基础设施可复用：

### 可复用组件

| 组件 | 位置 | 说明 |
|-----|------|------|
| Turn 取消机制 | `query_ops.rs:cancel_turn()` | 已有 `turn_control.cancel()` 调用 |
| Slot 状态管理 | `types.rs:SlotExecutionState` | `Idle/Running/Transitioning` 状态机 |
| Tape 事件系统 | `session-tape/entry.rs` | `TapeEntry::event()` 支持自定义事件 |
| SSE 广播 | `sse/mod.rs` | `broadcast::Sender<SsePayload>` 机制 |
| Pending Question | `types.rs:pending_question_waiters` | 问题等待和取消逻辑 |

### 需要新增

| 组件 | 说明 |
|-----|------|
| `message_queue` 字段 | `SessionSlot` 新增队列 |
| `interrupt_requested` 字段 | `SessionSlot` 新增中断标志 |
| `QueueMessage` 等命令 | `SessionCommand` 新增 |
| `SubmitTurn` 行为变更 | Running 时入队而非报错 |
| `handle_runtime_return` 变更 | 检查队列并自动开始新 turn |
| 新 SSE 事件 | `MessageQueued` 等 |

## Proposal

### 核心概念：消息队列

每个 session 维护一个消息队列：

```
┌─────────────────────────────────────────────────────┐
│                    Session State                     │
├─────────────────────────────────────────────────────┤
│  Status: Idle | Running                              │
│  Current Turn: Option<TurnHandle>                    │
│  Message Queue: Vec<QueuedMessage>                   │
│  Interrupt Flag: bool                                │
└─────────────────────────────────────────────────────┘
```

### 数据模型变更

#### types.rs 新增结构

```rust
// 新增：排队消息定义
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct QueuedMessage {
    pub id: String,           // 唯一标识，用于删除
    pub content: String,      // 消息内容
    pub queued_at_ms: u64,    // 入队时间戳
}

// 新增：队列操作错误（扩展 RuntimeWorkerError）
impl RuntimeWorkerError {
    pub fn queue_full(max_size: usize) -> Self {
        Self::bad_request(format!("message queue is full (max {} messages)", max_size))
    }

    pub fn message_not_found(id: &str) -> Self {
        Self::not_found(format!("message not found: {}", id))
    }

    pub fn cannot_modify_queue_while_running() -> Self {
        Self::bad_request("cannot modify message queue while session is running")
    }
}
```

#### SessionSlot 变更

```rust
// session_manager/types.rs

pub(crate) struct SessionSlot {
    // === 现有字段 ===
    pub(crate) session_path: PathBuf,
    pub(crate) provider_binding: SessionProviderBinding,
    pub(crate) history: Arc<RwLock<Vec<TurnLifecycle>>>,
    pub(crate) current_turn: Arc<RwLock<Option<CurrentTurnSnapshot>>>,
    pub(crate) context_stats: Arc<RwLock<ContextStats>>,
    pub(crate) execution: SlotExecutionState,
    pub(crate) pending_question_waiters: HashMap<String, tokio_oneshot::Sender<QuestionResult>>,

    // === 新增字段 ===
    /// 排队的消息
    pub(crate) message_queue: Vec<QueuedMessage>,

    /// 中断标志（用户按了 ESC）
    pub(crate) interrupt_requested: bool,
}
```

#### SessionCommand 新增

```rust
// session_manager/types.rs

pub(crate) enum SessionCommand {
    // === 现有命令 ===
    ListSessions { .. },
    CreateSession { .. },
    DeleteSession { .. },
    SubmitTurn { .. },
    CancelTurn { .. },
    // ...

    // === 新增命令 ===
    /// 发送消息（空闲时立即执行，运行时入队）
    QueueMessage {
        session_id: SessionId,
        content: String,
        reply: oneshot::Sender<Result<QueueMessageResponse, RuntimeWorkerError>>,
    },

    /// 获取当前消息队列
    GetQueue {
        session_id: SessionId,
        reply: oneshot::Sender<Result<Vec<QueuedMessage>, RuntimeWorkerError>>,
    },

    /// 删除队列中的消息
    DeleteQueuedMessage {
        session_id: SessionId,
        message_id: String,
        reply: oneshot::Sender<Result<(), RuntimeWorkerError>>,
    },

    /// 打断当前 turn
    InterruptTurn {
        session_id: SessionId,
        reply: oneshot::Sender<Result<bool, RuntimeWorkerError>>,
    },

    /// 处理队列中的消息（内部命令，由 handle_runtime_return 触发）
    SubmitQueuedMessages {
        session_id: SessionId,
        messages: Vec<String>,
        reply: oneshot::Sender<Result<String, RuntimeWorkerError>>,
    },
}

// 新增：QueueMessage 返回结果
#[derive(Serialize)]
pub struct QueueMessageResponse {
    pub status: QueueMessageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueMessageStatus { Started, Queued }
```

### API 端点

#### POST /api/session/message

发送消息。如果 session 空闲则立即开始 turn；如果正在运行则入队等待。

请求：
```json
{
  "message": "Also check the test file"
}
```

响应（立即执行）：
```json
{
  "status": "started",
  "turn_id": "turn_123"
}
```

响应（入队等待）：
```json
{
  "status": "queued",
  "position": 1,
  "message_id": "msg_abc123"
}
```

响应（队列已满）：
```json
{
  "error": "message queue is full (max 10 messages)"
}
```

**路由实现**：
```rust
// routes/session/handlers.rs

pub(crate) async fn send_message(
    State(state): State<SharedState>,
    Json(body): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let session_id = match require_session_id(state.as_ref(), body.session_id).await {
        Ok(id) => id,
        Err(response) => return response,
    };

    match state.session_manager.queue_message(session_id, body.message).await {
        Ok(response) => {
            let status = match response.status {
                QueueMessageStatus::Started => StatusCode::ACCEPTED,
                QueueMessageStatus::Queued => StatusCode::OK,
            };
            (status, Json(serde_json::to_value(response).unwrap()))
        }
        Err(error) => runtime_worker_error_response(error),
    }
}
```

#### POST /api/session/interrupt

立即打断当前 turn。用户按 ESC 时调用。

请求：
```json
{}
```

响应：
```json
{
  "interrupted": true,
  "turn_id": "turn_123"
}
```

**路由实现**：复用现有 `cancel_turn` 逻辑，但增加中断标记。

#### GET /api/session/queue

获取当前消息队列。

响应：
```json
{
  "messages": [
    {
      "id": "msg_1",
      "content": "Also check the test file",
      "queued_at_ms": 1711670400000
    }
  ]
}
```

#### DELETE /api/session/queue/{message_id}

删除队列中的指定消息。只能在 session 空闲时删除。

响应：
```json
{
  "deleted": true,
  "message_id": "msg_1"
}
```

### 状态转换

> 历史说明：这是提案阶段的概念图，用来解释 queue / interrupt 主线，不保证逐项映射当前实现中的具体类型名与内部状态字段。

```
                    ┌──────────┐
                    │   Idle   │
                    └────┬─────┘
                         │ Message
                         ▼
┌──────────────────────────────────────────────┐
│                  Running                      │
│                                              │
│  ┌─────────────┐     ┌──────────────────┐   │
│  │   Turn      │     │  Message Queue   │   │
│  │  Execution  │────▶│  (waiting)       │   │
│  └──────┬──────┘     └──────────────────┘   │
│         │                                    │
│         │ Interrupt (ESC)                    │
│         ▼                                    │
│  ┌─────────────┐                            │
│  │   Abort     │                            │
│  │  Now        │                            │
│  └─────────────┘                            │
└──────────────────────────────────────────────┘
                         │ Turn Complete / Abort
                         ▼
                    ┌──────────┐
                    │   Idle   │
                    │(process  │
                    │ queue)   │
                    └──────────┘
```

### 消息入队时机

| SlotStatus | 额外条件 | 发送消息行为 | 实现位置 |
|------------|---------|------------|----------|
| Idle | - | 立即开始新 turn | `submit_turn()` 正常调用 |
| Running | tool 执行中 | 入队 | `handle_queue_message` |
| Running | chat 生成中 | 入队 | `handle_queue_message` |
| Running | 有 pending question | **拒绝普通 turn**，由 pending question 控制面接管 | question / turn 路由约束 |

**注意**：`waiting_for_question` 是 turn / SSE 语义，不是独立 `SlotStatus`。当前代码里，如果 session 正在等待 question 回答，普通消息不会继续走队列主路径，而是先要求用户回答或取消当前问题。

### 消息入队实现

> 历史说明：下面这段提案伪代码把 `Running` 时的入队描述成“立刻追加 `message_queued` 到 tape”。当前实现已经改成更保守的版本：运行中普通入队只写内存 `message_queue` 并广播 `message_queued` SSE，不在这一刻直接 append jsonl。真正会稳定落盘的是后续的 `message_deleted` / `message_dequeued` 等事实，所以这段代码不能直接当作当前 crash-safe 持久化行为说明。

```rust
// session_manager/mod.rs

async fn handle_queue_message(
    &mut self,
    session_id: &str,
    content: String,
) -> Result<QueueMessageResponse, RuntimeWorkerError> {
    let slot = self.slots.get_mut(session_id).ok_or_else(|| {
        RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
    })?;

    // 检查队列是否已满
    const MAX_QUEUE_SIZE: usize = 10;
    if slot.message_queue.len() >= MAX_QUEUE_SIZE {
        return Err(RuntimeWorkerError::queue_full(MAX_QUEUE_SIZE));
    }

    match slot.status() {
        SlotStatus::Idle => {
            // 空闲时立即开始 turn
            // 注意：TurnExecutionService 是 async，需要在 async context 中调用
            let mut turn_execution = TurnExecutionService::new(
                &mut self.slots,
                &self.config,
                &self.return_tx,
            );
            let turn_id = turn_execution.submit_turn(session_id, content).await?;
            Ok(QueueMessageResponse {
                status: QueueMessageStatus::Started,
                turn_id: Some(turn_id),
                position: None,
                message_id: None,
            })
        }
        SlotStatus::Running => {
            // 运行时入队
            let message_id = generate_message_id();
            let queued_at_ms = now_timestamp_ms();

            // 追加 tape 事件
            let entry = TapeEntry::event("message_queued", Some(json!({
                "id": message_id,
                "content": content,
                "queued_at_ms": queued_at_ms,
            })));
            SessionTape::append_jsonl_entry(&slot.session_path, &entry)?;

            // 更新内存状态
            let position = slot.message_queue.len() as u32 + 1;
            slot.message_queue.push(QueuedMessage {
                id: message_id.clone(),
                content,
                queued_at_ms,
            });

            // 广播 SSE 事件
            let _ = self.config.broadcast_tx.send(SsePayload::MessageQueued {
                session_id: session_id.to_string(),
                message_id: message_id.clone(),
                position,
                content_preview: content.chars().take(50).collect(),
            });

            Ok(QueueMessageResponse {
                status: QueueMessageStatus::Queued,
                turn_id: None,
                position: Some(position),
                message_id: Some(message_id),
            })
        }
    }
}
```

### ESC 打断行为

用户按 ESC 时：
1. 设置 `interrupt_requested = true`
2. 调用现有 `turn_control.cancel()` 取消 turn
3. 如果有 pending question，一并取消
4. Turn 结束后检查队列，如有消息则开始新 turn

```rust
// session_manager/mod.rs

fn handle_interrupt_turn(
    &mut self,
    session_id: &str,
) -> Result<bool, RuntimeWorkerError> {
    let slot = self.slots.get_mut(session_id).ok_or_else(|| {
        RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
    })?;

    if slot.status() != SlotStatus::Running {
        return Ok(false);
    }

    // 设置中断标志
    slot.interrupt_requested = true;

    // 复用现有取消逻辑 - 通过 SessionQueryService
    // 而不是直接调用 running_turn.control.cancel()
    // 这样可以正确处理 pending question
    let turn_id = read_lock(&slot.current_turn)
        .as_ref()
        .map(|t| t.turn_id.clone());

    if let Some(running_turn) = slot.running_turn() {
        running_turn.control.cancel();
    }

    // 取消 pending question（如果有）
    // 注意：pending question 的取消由 cancel_turn 逻辑处理
    // 这里设置标志后，turn 结束时会正确清理

    // 广播 SSE 事件
    let _ = self.config.broadcast_tx.send(SsePayload::TurnInterrupted {
        session_id: session_id.to_string(),
        turn_id,
    });

    Ok(true)
}
```

### Turn 完成后队列处理

修改 `handle_runtime_return()`：

```rust
// session_manager/mod.rs

fn handle_runtime_return(&mut self, mut ret: RuntimeReturn) {
    if let Some(slot) = self.slots.get_mut(&ret.session_id) {
        // === 现有逻辑 ===
        let session_path = slot.session_path.clone();
        if let Err(error) = refresh_runtime_tape_from_disk(&session_path, &mut ret.runtime) {
            // ... 错误处理
        }
        // ... 其他同步逻辑
        
        // 保存中断标志（在 finish_turn 之前）
        let interrupt_requested = slot.interrupt_requested;
        slot.interrupt_requested = false;  // 重置标志

        // 完成当前 turn（状态从 Running -> Idle）
        if let Err(error) = slot.finish_turn(ret.runtime, ret.subscriber) {
            // ... 错误处理
            return;  // 重要：出错时不再处理队列
        }

        // === 新增：检查队列（此时 status 已是 Idle）===
        if !slot.message_queue.is_empty() {
            let messages = self.drain_queue(&ret.session_id, slot);

            if let Some(contents) = messages {
                // 广播队列处理事件
                let _ = self.config.broadcast_tx.send(SsePayload::QueueProcessing {
                    session_id: ret.session_id.clone(),
                    count: contents.len() as u32,
                });

                // 需要通过 command_tx 发送新命令来开始 turn
                // 不能直接调用 async 方法（handle_runtime_return 是 sync）
                let _ = self.command_tx.try_send(SessionCommand::SubmitQueuedMessages {
                    session_id: ret.session_id.clone(),
                    messages: contents,
                });
            }
        }
    }
}

fn drain_queue(
    &mut self,
    session_id: &str,
    slot: &mut SessionSlot,
) -> Option<Vec<String>> {
    if slot.message_queue.is_empty() {
        return None;
    }

    let messages: Vec<QueuedMessage> = slot.message_queue.drain(..).collect();

    // 追加 dequeued 事件到 tape
    for msg in &messages {
        let entry = TapeEntry::event("message_dequeued", Some(json!({
            "id": msg.id
        })));
        SessionTape::append_jsonl_entry(&slot.session_path, &entry);
    }

    // 返回消息内容列表
    Some(messages.iter().map(|m| m.content.clone()).collect())
}
```

**重要修正**：`handle_runtime_return` 是同步方法，不能直接 `.await`。
正确做法是通过 `command_tx` 发送新的 `SessionCommand` 来触发队列处理。

在 `handle_command` 中处理 `SubmitQueuedMessages`：

```rust
// session_manager/mod.rs

async fn handle_command(&mut self, command: SessionCommand) {
    match command {
        // ... 现有命令处理 ...
        
        SessionCommand::SubmitQueuedMessages { session_id, messages, reply } => {
            let mut turn_execution = TurnExecutionService::new(
                &mut self.slots,
                &self.config,
                &self.return_tx,
            );
            let result = turn_execution.submit_turn_multi(&session_id, messages).await;
            let _ = reply.send(result);
        }
    }
}
```

### SSE 事件新增

```rust
// sse/mod.rs

#[derive(Clone)]
pub enum SsePayload {
    // === 现有事件 ===
    Stream { .. },
    Status { .. },
    CurrentTurnStarted { .. },
    TurnCompleted { .. },
    TurnCancelled { .. },
    ContextCompressed { .. },
    SyncRequired { .. },
    Error { .. },
    SessionCreated { .. },
    SessionUpdated { .. },
    SessionDeleted { .. },

    // === 新增事件 ===
    /// 消息入队
    MessageQueued {
        session_id: String,
        message_id: String,
        position: u32,
        content_preview: String,
    },

    /// 消息从队列删除
    MessageDeleted {
        session_id: String,
        message_id: String,
        remaining_count: u32,
    },

    /// Turn 被打断
    TurnInterrupted {
        session_id: String,
        turn_id: Option<String>,
    },

    /// 队列开始处理
    QueueProcessing {
        session_id: String,
        count: u32,
    },
}

// 新增序列化结构
#[derive(Serialize)]
struct MessageQueuedData {
    session_id: String,
    message_id: String,
    position: u32,
    content_preview: String,
}

#[derive(Serialize)]
struct MessageDeletedData {
    session_id: String,
    message_id: String,
    remaining_count: u32,
}

#[derive(Serialize)]
struct TurnInterruptedData {
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    turn_id: Option<String>,
}

#[derive(Serialize)]
struct QueueProcessingData {
    session_id: String,
    count: u32,
}
```

### Session Tape 设计

追加事件：

```json
// 消息入队
{
  "id": 123,
  "kind": "event",
  "payload": {
    "name": "message_queued",
    "data": {
      "id": "msg_abc123",
      "content": "Also check the test file",
      "queued_at_ms": 1711670400000
    }
  },
  "date": "2026-03-29T10:00:00Z"
}

// 消息删除
{
  "id": 124,
  "kind": "event",
  "payload": {
    "name": "message_deleted",
    "data": {
      "id": "msg_abc123"
    }
  },
  "date": "2026-03-29T10:00:05Z"
}

// 消息出队（开始处理）
{
  "id": 125,
  "kind": "event",
  "payload": {
    "name": "message_dequeued",
    "data": {
      "id": "msg_abc123"
    }
  },
  "date": "2026-03-29T10:01:00Z"
}

// Turn 被打断
{
  "id": 126,
  "kind": "event",
  "payload": {
    "name": "turn_interrupted",
    "data": {
      "turn_id": "turn_xyz",
      "reason": "user_esc"
    }
  },
  "date": "2026-03-29T10:02:00Z"
}
```

### 持久化与恢复

> 历史说明：本节保留的是提案阶段对“完整 queue 事件链落盘后可恢复”的设计说明。当前代码里，`restore_queue_from_tape(...)` 的确存在，也会根据 `message_queued` / `message_deleted` / `message_dequeued` 重建队列；但运行中普通入队路径目前不会立即写入 `message_queued`。所以“重启恢复”目前只对**已经落盘**的 queue 事件成立，不应解读为“所有运行中的临时入队消息都具备无损 crash-recovery”。

#### 恢复逻辑

Server 启动加载 session 时（`SessionSlotFactory::create`）：

```rust
// session_manager/mod.rs

fn restore_queue_from_tape(tape: &SessionTape) -> Vec<QueuedMessage> {
    let mut queue: Vec<QueuedMessage> = Vec::new();
    let mut deleted: HashSet<String> = HashSet::new();

    for entry in tape.entries() {
        if entry.kind != "event" {
            continue;
        }

        let event_name = entry.event_name();
        let event_data = entry.event_data();

        match event_name {
            Some("message_queued") => {
                if let Some(data) = event_data {
                    if let Ok(msg) = parse_queued_message(data) {
                        // 只有未被删除的才加入
                        if !deleted.contains(&msg.id) {
                            queue.push(msg);
                        }
                    }
                }
            }
            Some("message_deleted") | Some("message_dequeued") => {
                if let Some(id) = event_data.and_then(|d| d.get("id")).and_then(|v| v.as_str()) {
                    deleted.insert(id.to_string());
                    queue.retain(|m| m.id != id);
                }
            }
            _ => {}
        }
    }

    queue
}

fn parse_queued_message(data: &Value) -> Result<QueuedMessage, ()> {
    Ok(QueuedMessage {
        id: data.get("id").and_then(|v| v.as_str()).ok_or(())?.to_string(),
        content: data.get("content").and_then(|v| v.as_str()).ok_or(())?.to_string(),
        queued_at_ms: data.get("queued_at_ms").and_then(|v| v.as_u64()).ok_or(())?,
    })
}
```

#### SessionSlotFactory 变更

```rust
// session_manager/mod.rs

impl<'a> SessionSlotFactory<'a> {
    fn create(&self, session_id: &str) -> Result<SessionSlot, RuntimeWorkerError> {
        let session_path = self.config.sessions_dir.join(format!("{session_id}.jsonl"));
        let mut tape = load_session_tape_with_repair(&session_path)?;
        // ... 现有逻辑 ...

        // === 新增：恢复队列 ===
        let message_queue = restore_queue_from_tape(&tape);

        Ok(SessionSlot::idle(
            session_path,
            provider_binding,
            Arc::new(RwLock::new(snapshots.history)),
            Arc::new(RwLock::new(snapshots.current_turn)),
            Arc::new(RwLock::new(context_stats)),
            runtime,
            subscriber,
            // === 新增参数 ===
            message_queue,
        ))
    }
}
```

### 队列消息处理

Turn 结束后检查队列，**一次性取出全部消息**，每条作为独立的 user message 追加到 tape。

#### Runtime 新增方法

```rust
// agent-runtime/src/runtime/turn/driver.rs

impl<M, T> AgentRuntime<M, T>
where
    M: LanguageModel,
    T: ToolExecutor,
{
    /// 处理多条用户消息的 turn
    /// 
    /// 每条消息作为独立的 user message 追加到 tape，
    /// agent 能清晰看到这是多条独立的用户输入。
    pub async fn handle_turn_streaming_multi(
        &mut self,
        user_inputs: Vec<String>,
        control: TurnControl,
        on_delta: impl FnMut(StreamEvent) + Send,
    ) -> Result<TurnOutput, RuntimeError> {
        let turn_id = next_turn_id();
        let started_at_ms = now_timestamp_ms();
        let abort_signal = control.abort_signal();

        self.ensure_agent_started()?;

        let mut llm_step_index = 0_u32;

        if abort_signal.is_aborted() {
            return Err(RuntimeError::cancelled());
        }

        self.maybe_auto_compress_current_context(&turn_id, &mut llm_step_index).await;

        if abort_signal.is_aborted() {
            return Err(RuntimeError::cancelled());
        }

        // === 关键差异：追加多条 user message ===
        let mut last_entry_id = None;
        for user_input in &user_inputs {
            let user_input = self.rewrite_input(user_input.clone())?;
            let user_message = Message::new(Role::User, user_input);
            last_entry_id = Some(
                self.append_tape_entry(TapeEntry::message(&user_message).with_run_id(&turn_id))?
            );
            self.publish_event(RuntimeEvent::UserMessage { 
                content: user_message.content.clone() 
            });
        }

        let user_entry_id = last_entry_id.ok_or_else(|| {
            RuntimeError::session("no user messages provided")
        })?;
        let first_user_content = user_inputs.first().cloned().unwrap_or_default();
        
        let buffers = TurnBuffers::new(user_entry_id);
        let first_message = Message::new(Role::User, first_user_content);
        self.notify_turn_start(&turn_id, &first_message.content);

        self.drive_turn_loop(
            turn_id,
            started_at_ms,
            first_message, // 用于错误报告
            control,
            buffers,
            llm_step_index,
            on_delta,
        )
        .await
    }
}
```

#### Tape 中的效果

```json
// 三条独立 user message
{"id": 100, "kind": "message", "payload": {"role": "user", "content": "Check the test file"}, "meta": {"run_id": "turn_123"}}
{"id": 101, "kind": "message", "payload": {"role": "user", "content": "Focus on auth module"}, "meta": {"run_id": "turn_123"}}
{"id": 102, "kind": "message", "payload": {"role": "user", "content": "Also check error handling"}, "meta": {"run_id": "turn_123"}}
{"id": 103, "kind": "assistant", "payload": {...}, "meta": {"run_id": "turn_123"}}
```

Agent 会看到三条独立的 user message，上下文清晰。

#### TurnExecutionService 新增方法

> 历史说明：这里保留的是提案时的多消息 worker 设计稿。当前实现最终收口为 `submit_turn(session_id, prompts: Vec<String>)` 这类统一入口，而不是单独长期保留一个 `submit_turn_multi(...)` 公开接口。

```rust
// session_manager/turn_execution.rs

impl<'a> TurnExecutionService<'a> {
    /// 提交包含多条用户消息的 turn
    pub(super) async fn submit_turn_multi(
        &mut self,
        session_id: &str,
        prompts: Vec<String>,
    ) -> Result<String, RuntimeWorkerError> {
        let slot = self.slots.get_mut(session_id).ok_or_else(|| {
            RuntimeWorkerError::not_found(format!("session not found: {session_id}"))
        })?;

        let (runtime, subscriber, running_turn) = slot.begin_turn()?;
        *write_lock(&slot.context_stats) = runtime.context_stats();
        let turn_control = running_turn.control.clone();

        // ... 类似 submit_turn 的设置逻辑 ...

        let worker = TurnWorker::new_multi(
            runtime,
            subscriber,
            prompts,
            turn_control,
            context,
        );

        // ... spawn 并返回 turn_id
    }
}

// TurnWorker 新增模式
enum TurnWorkerMode {
    Submit { prompt: String },
    SubmitMulti { prompts: Vec<String> },
}

impl TurnWorker {
    pub(super) fn new_multi(
        runtime: AgentRuntime<ServerModel, ToolRegistry>,
        subscriber: RuntimeSubscriberId,
        prompts: Vec<String>,
        turn_control: agent_runtime::TurnControl,
        context: TurnWorkerContext,
    ) -> Self {
        Self { 
            runtime, 
            subscriber, 
            mode: TurnWorkerMode::SubmitMulti { prompts }, 
            turn_control, 
            context 
        }
    }

    pub(super) async fn run(mut self) -> RuntimeReturn {
        // ... 根据 mode 调用 handle_turn_streaming 或 handle_turn_streaming_multi ...
        match &self.mode {
            TurnWorkerMode::Submit { prompt } => {
                self.runtime.handle_turn_streaming(prompt.clone(), self.turn_control.clone(), on_delta).await
            }
            TurnWorkerMode::SubmitMulti { prompts } => {
                self.runtime.handle_turn_streaming_multi(prompts.clone(), self.turn_control.clone(), on_delta).await
            }
        }
        // ...
    }
}
```

**为什么每条作为独立 user message**：
1. Agent 能清晰区分每条用户输入
2. Tape 语义准确——每条消息都是独立的用户输入
3. 避免分隔符冲突问题
4. 便于追踪每条消息的处理状态

### 并发控制

> 历史说明：这一节保留了提案阶段的简化矩阵。当前代码里，“等待 question 时的普通消息处理”优先遵守 question 控制面约束，而不是简单等同于“继续入队”。

| SlotStatus | 额外条件 | Message | Interrupt | Delete Queue |
|------------|---------|---------|-----------|--------------|
| Idle | - | ✓ 立即开始 turn | ✗ 返回 false | ✓ 允许 |
| Running | - | ✓ 入队 | ✓ 打断 | ✗ 返回错误 |
| Running | 有 pending question | 以 pending question 控制面优先，不直接作为普通消息入队保证 | ✓ 打断+取消问题 | ✗ 返回错误 |

**注意**：`WaitingForQuestion` 是 `TurnStatus`，不是 `SlotStatus`。

### 前端展示设计

#### 队列消息 UI

当有排队消息时，在输入框上方显示队列区域：

```
┌─────────────────────────────────────────────┐
│  [Agent is running...]                      │
├─────────────────────────────────────────────┤
│  📋 Queued messages (2):                    │
│  ┌───────────────────────────────────────┐  │
│  │ 1. "Also check the test file"    [×]  │  │
│  └───────────────────────────────────────┘  │
│  ┌───────────────────────────────────────┐  │
│  │ 2. "Focus on auth module"        [×]  │  │
│  └───────────────────────────────────────┘  │
├─────────────────────────────────────────────┤
│  [Type a message...]              [Send]    │
│                              [Press ESC to] │
│                                [interrupt]  │
└─────────────────────────────────────────────┘
```

#### 交互行为

1. **显示队列**：通过 SSE `MessageQueued` 事件更新，显示消息预览（前 50 字符）
2. **删除消息**：点击 [×] 调用 DELETE API（仅在 Idle 时可用）
3. **队列处理中**：显示 "Processing queued messages (3)..."
4. **空队列**：队列区域隐藏

## Risks and Mitigations

### 1. 队列无限增长

**决策**：设置最大队列长度 10 条，超出时返回错误拒绝新消息。

**理由**：
- 用户明确知道消息被拒绝，可以自行决定删除哪条
- 避免静默丢弃导致用户困惑

### 2. 打断导致部分工作丢失

**决策**：打断时保存已生成的部分输出到 tape。

**实现**：
- 已完成的 tool 调用结果已经在 tape 中
- Chat 部分输出：在 turn 取消时，将已生成的文本作为 `partial_output` 追加到 tape
- 追加 `turn_interrupted` 事件标记中断点

```rust
// turn_execution.rs

async fn handle_terminal_events(&mut self, error: Option<RuntimeError>) {
    // ... 现有逻辑 ...

    if let Some(error) = &error {
        if error.is_cancelled() {
            // === 新增：保存部分输出 ===
            if let Some(partial) = self.collect_partial_output() {
                let entry = TapeEntry::event("partial_output", Some(json!({
                    "turn_id": self.context.turn_id,
                    "content": partial,
                    "reason": "interrupted"
                })));
                // 追加到 tape
            }

            // 追加中断事件
            let entry = TapeEntry::event("turn_interrupted", Some(json!({
                "turn_id": self.context.turn_id,
                "reason": "user_esc"
            }))).with_run_id(&self.context.turn_id);
            // ...
        }
    }
}
```

### 3. 与 Pending Question 的冲突

**决策**：Interrupt 同时取消 pending question。

**行为**：
- 用户按 ESC → 设置 `interrupt_requested = true` → 取消 turn → 取消 pending question
- QuestionResult 状态设为 `Cancelled`
- 广播 `TurnInterrupted` SSE 事件

### 4. 队列消息引用已压缩上下文

**场景**：用户入队消息 "检查上面提到的文件"，然后执行 handoff 压缩上下文。

**决策**：队列保留，不清空。

**理由**：
- 队列消息是待执行的指令，不是上下文的一部分
- 用户需自行判断是否删除无效的队列消息

**缓解**：handoff 创建时，如果有排队消息，在 SSE 事件中提醒：

```rust
// 如果有队列消息，在 HandoffCreated 事件中包含队列大小
SsePayload::HandoffCreated {
    session_id,
    anchor_entry_id,
    queue_size: slot.message_queue.len() as u32,
}
```

## Alternatives Considered

### 方案 A：Steal 立即打断

用户发消息直接打断当前 turn。

**不采纳**：太激进，用户可能只是想追加指令，不想打断当前工作。

### 方案 B：保留 Followup + Steal 双机制

Followup 入队，Steal 打断。

**不采纳**：两个概念增加复杂度，用户需要思考用哪个。

### 方案 C：队列存 SQLite

**不采纳**：
- 破坏 tape 语义一致性
- 删除需要额外操作
- 恢复时需要两个数据源

## Open Questions

### 1. 队列最大长度是否可配置

**当前决策**：固定 10 条。

**后续考虑**：如果用户反馈需要调整，可在 `aia-config` 中添加配置项。

### 2. Channel 消息是否需要特殊处理

**当前决策**：Channel 消息与用户消息同等对待，共用同一队列。

**后续考虑**：如果需要区分优先级，可扩展 `QueuedMessage.source` 字段。

## Rollout Plan

> 历史说明：下面的 Phase / Checklist 是 RFC 落地时的实施清单，很多条目已经完成；其中凡涉及“重启后恢复 queue”的表述，都应按上面的 `Implementation Snapshot` 理解为“基于已落盘事件的恢复能力”，而不是当前实现已经提供完整的运行中即时入队持久化保证。

### Phase 1：数据模型与队列基础

**改动文件**：
- `session_manager/types.rs` - 新增 `QueuedMessage`、`QueueMessageResponse`，修改 `SessionSlot` 和 `SessionCommand`
- `session_manager/mod.rs` - 新增 `handle_queue_message`、`handle_get_queue`、`handle_delete_queued_message`
- `routes/session/mod.rs` - 新增路由
- `routes/session/handlers.rs` - 新增 handlers

**功能**：
- `POST /api/session/message` 支持入队
- `GET /api/session/queue` 查询队列
- `DELETE /api/session/queue/{message_id}` 删除消息

### Phase 2：Turn 完成后队列处理

**改动文件**：
- `session_manager/types.rs` - 新增 `SubmitQueuedMessages` 命令
- `session_manager/mod.rs` - 修改 `handle_runtime_return`，新增 `drain_queue`，处理 `SubmitQueuedMessages`
- `session_manager/turn_execution.rs` - 新增 `submit_turn_multi`，修改 `handle_terminal_events` 保存部分输出

**功能**：
- Turn 结束后自动检查队列
- 多条消息作为独立 user message 处理

### Phase 3：中断机制

**改动文件**：
- `session_manager/types.rs` - 新增 `InterruptTurn` 命令
- `session_manager/mod.rs` - 新增 `handle_interrupt_turn`
- `sse/mod.rs` - 新增 `TurnInterrupted`、`MessageQueued` 等事件
- `routes/session/handlers.rs` - 新增 `interrupt_turn` handler

**功能**：
- `POST /api/session/interrupt` 打断 turn
- 中断标志和队列联动

### Phase 4：Tape 持久化与恢复

**改动文件**：
- `session_manager/mod.rs` - 新增 `restore_queue_from_tape`，修改 `SessionSlotFactory`

**功能**：
- 基于已落盘 queue 事件的重建 / 恢复
- queue 相关 tape 事件链路建立（但 `Running` 态即时入队仍有未落盘窗口）

### Phase 5：UI 集成

**前端改动**：
- 输入框始终可用（不再在 Running 时禁用）
- 添加队列消息显示区域
- 添加 ESC 快捷键打断
- 显示队列状态指示器

## Success Criteria

1. 用户可在 agent 执行时发送消息
2. 消息在当前 turn 运行时入队
3. 用户按 ESC 可立即打断 turn
4. Turn 结束后自动处理队列消息
5. 若 queue 事件已经落盘，Server 重启后队列状态可恢复；当前 `Running` 态即时入队仍存在崩溃丢失窗口
6. 所有操作有清晰的 SSE 事件反馈
7. 队列满时返回明确错误而非静默丢弃

## Implementation Checklist

### 数据模型改动

- [x] `types.rs`: 新增 `QueuedMessage` 结构
- [x] `types.rs`: 新增 `QueueMessageResponse` 结构和 `QueueMessageStatus` 枚举
- [x] `types.rs`: `SessionSlot` 新增 `message_queue` 和 `interrupt_requested` 字段
- [x] `types.rs`: `SessionCommand` 新增 `QueueMessage`、`GetQueue`、`DeleteQueuedMessage`、`InterruptTurn`、`SubmitQueuedMessages`
- [x] `types.rs`: `SessionSlot::idle()` 新增 `message_queue` 参数

### Session Manager 层改动

- [x] `mod.rs`: 实现 `handle_queue_message` (async)
- [x] `mod.rs`: 实现 `handle_get_queue`
- [x] `mod.rs`: 实现 `handle_delete_queued_message`
- [x] `mod.rs`: 实现 `handle_interrupt_turn`
- [x] `mod.rs`: 修改 `handle_runtime_return`，通过 `command_tx` 触发队列处理
- [x] `mod.rs`: 实现 `drain_queue` 返回 `Vec<String>`
- [x] `mod.rs`: 实现 `restore_queue_from_tape`（针对已落盘的 queue 事件）
- [x] `mod.rs`: 修改 `SessionSlotFactory::create` 恢复队列（不代表 `Running` 态即时入队已完整持久化）
- [x] `mod.rs`: 在 `handle_command` 中处理 `SubmitQueuedMessages` 命令
- [x] `turn_execution.rs`：队列消息通过已有 `submit_turn(session_id, prompts: Vec<String>)` 统一提交，不存在独立的 `submit_turn_multi` 接口
- [x] `turn_execution.rs`：不存在 `TurnWorkerMode::Multi` 变体

### SSE 层改动

- [x] `sse/mod.rs`: 新增 `MessageQueued`、`MessageDeleted`、`TurnInterrupted`、`QueueProcessing` 事件
- [x] `sse/mod.rs`: 新增对应的 `*Data` 序列化结构
- [x] `sse/mod.rs`: 在 `into_axum_event` 中处理新事件

### 路由层改动

- [x] `routes/session/mod.rs`: 新增路由定义
- [x] `routes/session/handlers.rs`: 新增 `send_message`、`get_queue`、`delete_queued_message`
- [x] `routes/session/handlers.rs`: 新增 `interrupt_turn`

### Runtime 层改动

- [x] `agent-runtime/src/runtime/turn/driver.rs`: 新增 `handle_turn_streaming_multi` 方法
- [ ] `agent-runtime/src/runtime/turn/types.rs`: 如需调整 TurnBuffers 以支持多条消息 (未需要)

### 测试用例

- [x] 入队消息后验证 `position` 正确递增
- [x] 入队消息后再发一条，验证 `position` 正确
- [x] 打断 turn 后，队列消息被正确处理
- [x] Server 重启后，队列可从**已存在的** tape queue 事件恢复
- [x] 队列满时拒绝新消息（返回错误）
- [x] 在 Idle 时删除队列消息
- [ ] 在 Running 时删除队列消息返回错误 (需要更复杂的测试设置)
- [ ] 并发入队的线程安全 (需要更复杂的测试设置)
- [ ] 有 pending question 时入队消息 (需要更复杂的测试设置)
- [ ] 打断时有 pending question，两者都被取消 (需要更复杂的测试设置)
- [x] `SubmitQueuedMessages` 命令正确处理
- [x] `handle_turn_streaming_multi` 正确追加多条 user message

### API 端点测试

- [x] `POST /api/session/message` 空闲时返回 `started`
- [x] `POST /api/session/message` 运行时返回 `queued`
- [ ] `POST /api/session/message` 队列满时返回错误 (需要 mock 或设置状态)
- [x] `GET /api/session/queue` 返回正确的队列
- [ ] `DELETE /api/session/queue/{id}` 正确删除 (需要 Idle 状态设置)
- [x] `POST /api/session/interrupt` 正确打断 turn
