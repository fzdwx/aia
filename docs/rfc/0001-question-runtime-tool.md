---
rfc: 0001
name: question-runtime-tool
title: Question Tool Suspend Boundary
description: Defines the current Question suspend boundary: builtin tool emits PendingToolRequest, runtime owns generic suspension, server owns question control-plane facts.
status: Implemented
date: 2026-03-25
authors:
  - aia
supersedes: []
superseded_by: null
---

# RFC 0001: `Question` Tool Suspend Boundary

## Summary

当前实现采用三层边界：

- `builtin-tools` 定义 `Question` 工具，并在执行时返回结构化 `PendingToolRequest(kind = question)`
- `agent-runtime` 只负责通用挂起原语：`tool_request_pending`、`turn_suspended`、`resume_turn_after_tool_result(...)`
- `apps/agent-server` 负责把通用挂起请求翻译成 question 控制面事实：`question_requested`、`question_resolved`、`GET|PUT|DELETE /api/session/question`

这份 RFC 记录的不是“最初想怎么做”，而是仓库当前已经落地并应继续遵守的实现边界。

## Motivation

`Question` 的本质不是“同步算一个结果”，而是“在当前 turn 中发起结构化澄清并等待用户输入”。如果把它做成运行时内部特判，会带来三个问题：

- `agent-runtime` 同时承担通用编排和交互控制面语义，边界变脏
- question 的发起、恢复、控制面会分裂在 runtime 与 server 两边
- 恢复链路会依赖 question 专属状态机，而不是共享的工具挂起原语

当前实现目标是把这个能力收口成：共享协议稳定、runtime 通用、server 负责交互事实与控制面。

## Goals

- 允许模型在 turn 中发起结构化问题
- 保持 `Question` 是否可见由 session capability 决定
- 让工具执行统一支持“完成 / 挂起”两种结果
- 让 question 的恢复与控制面通过 append-only tape 事实重建
- 保持 server 是交互式问题的唯一控制面权威来源

## Non-Goals

- 不做外部协议映射
- 不做文本降级版 question channel 交互
- 不在本轮设计多用户审批或投票流
- 不让运行时或界面自动替用户提交推荐项

## Current Design

### 1. 能力权威来源

`SessionInteractionCapabilities` 仍然是 `Question` 是否对模型可见的唯一权威来源。

职责分工保持为：

- `apps/agent-server` 计算并维护 session 级交互能力
- `agent-runtime` 只消费该能力来决定 visible tools
- `apps/web` 或其他客户端只消费这份能力做交互呈现

当前约束仍然成立：不支持交互式组件的 session 不暴露 `Question`。

### 2. `Question` 是普通 interactive builtin tool

`Question` 不再是 runtime 内建专属工具，而是 `builtin-tools` 中的普通 interactive tool。

它与其他工具的关键区别不是注册位置，而是返回值形状：

- 普通工具返回 `ToolCallOutcome::Completed`
- `QuestionTool` 返回 `ToolCallOutcome::Suspended`

挂起载荷使用共享类型 `PendingToolRequest`，并固定满足：

- `tool_name = "Question"`
- `kind = "question"`
- `payload` 可解码为 `QuestionRequest`

这意味着 question 的“特殊性”体现在共享工具结果协议里，而不是体现在 runtime 内部写死的 question 分支里。

### 3. 共享协议

`QuestionRequest`、`QuestionItem`、`QuestionOption`、`QuestionAnswer`、`QuestionResult` 仍然是共享协议的核心结构。

当前建议约束：

- `request_id` 是 pending question 的唯一主键
- `invocation_id` 必须与原始 `ToolCall` 一一对应
- `turn_id` 必须与挂起 turn 一一对应
- `recommended_option_id` 仍然是单值，而不是数组

`QuestionResult.status` 继续允许以下结构化状态：

- `answered`
- `cancelled`
- `dismissed`
- `timed_out`
- `unavailable`

其中真正进入当前主路径闭环的是：`answered` 与 `cancelled`；其余状态保留为协议扩展位。

### 4. runtime 只拥有通用挂起语义

`agent-runtime` 当前只关心工具执行会产生两种结果：

- `Completed { result }`
- `Suspended { request }`

当工具挂起时，runtime 只做三件事：

1. 记录原始 `tool_call`
2. 记录通用 `tool_request_pending`
3. 以 `turn_suspended` 和 `TurnOutcome::Suspended` 结束当前 turn

runtime 不再直接：

- 生成 `question_requested`
- 写 `turn_waiting_for_question`
- 暴露 `WaitingForQuestion`
- 提供 `resume_turn_after_question(...)`

恢复入口也已经通用化为 `resume_turn_after_tool_result(...)`。对 runtime 来说，它恢复的是“一个已挂起的工具调用”，而不是“一个 question”。

### 5. server 拥有 question 控制面语义

`apps/agent-server` 当前负责把通用挂起请求翻译为 question 事实和控制面。

具体来说：

- runtime 归还后，server 从 `tool_request_pending` 中挑出 `kind = question` 的挂起请求
- server 把该通用挂起请求解码为 `QuestionRequest`
- server 追加 `question_requested`
- `GET /api/session/question` 读取当前 pending question
- `PUT /api/session/question` 追加 `question_resolved + tool_result(Question)` 并驱动恢复
- `DELETE /api/session/question` 归一化为 `cancelled` 结果，仍走同一条落盘路径

这保证了 question 的控制面事实只在桥接层拥有，而不会回流污染 runtime。

### 6. tape 事实与落盘顺序

当前实现下，question 相关磁带事实分成两层：

第一层是通用挂起事实：

- `tool_call(Question)`
- `event(tool_request_pending)`
- `event(turn_suspended)`

第二层是 question 控制面事实：

- `event(question_requested)`
- `event(question_resolved)`
- `tool_result(Question)`

推荐理解为：

- runtime 负责记录“这个工具调用被挂起了”
- server 负责记录“这个挂起请求其实是一个 question，并且已经被回答/取消了”

### 7. 恢复逻辑

恢复分两段：

第一段是 question 控制面恢复：

- server 通过 `question_requested` / `question_resolved` 与 `request_id` 恢复当前 pending question

第二段是原 turn 恢复：

- 当回答或取消到达后，server 追加 `tool_result(Question)`
- 然后调用 runtime 的通用恢复入口 `resume_turn_after_tool_result(...)`

也就是说：

- pending question 是 server 控制面恢复出来的
- 原 turn continuation 是 runtime 通用挂起恢复出来的

这两个恢复面不再混在一起。

## Server Control Plane

当前控制面固定为：

- `GET /api/session/question`
- `PUT /api/session/question`
- `DELETE /api/session/question`

约束如下：

- 同一 session 同时最多只允许一个 pending question
- `request_id` 必须匹配当前 pending question
- 当 session 存在 pending question 时，普通 `POST /api/turn` 应被拒绝
- “查看 / 回答 / 取消当前 question” 不算新的普通 turn

## UI and Session Projection

UI 投影层仍可继续使用 `WaitingForQuestion` 作为展示语义，但这已经是 server / UI 层的派生状态，而不是 runtime 内部状态。

也就是说：

- runtime 输出的是通用 `Suspended`
- server 结合当前是否存在 pending question，把它投影成 question 等待态
- Web 继续在输入区或独立承接面展示 pending question composer

## Why This Boundary

采用当前边界有三个直接收益：

### 1. runtime 变干净

runtime 只拥有通用工具挂起原语，后续即使出现别的“需要等待外部输入”的工具，也可以复用同一套机制，而不是继续增加专名分支。

### 2. server 成为唯一控制面

question 的待答状态、恢复、冲突判断、API 入口都只由 server 持有，符合仓库“桥接层拥有控制面，runtime 只编排”的方向。

### 3. tape 事实更清晰

通用挂起事实与 question 控制面事实分层后，恢复逻辑、调试投影和将来的 trace 观察都会更稳定。

## Risks and Mitigations

### 1. 把 question 语义重新长回 runtime

缓解方式：新增交互式工具时，优先复用 `ToolCallOutcome::Suspended`，不要再把具体语义写进 runtime turn loop。

### 2. `tool_request_pending` 与 `question_requested` 漂移

缓解方式：server 从 runtime tape 中解码 question 挂起请求时，必须校验 `request_id`、`invocation_id`、`turn_id` 与 payload 一致，不允许盲信单层事实。

### 3. 恢复时误把别的挂起工具当成 question

缓解方式：只处理 `kind = question` 的 `PendingToolRequest`；其他挂起型工具未来应拥有各自桥接层翻译逻辑，而不是复用 question 控制面。

### 4. 旧磁带兼容

缓解方式：server 当前仍兼容旧的 `turn_waiting_for_question` 事实，以便旧会话数据平滑恢复；新路径统一以 `turn_suspended` 为主。

## Alternatives Considered

### 方案 A：继续把 `Question` 留在 runtime 内部特判

未采用。因为这会继续把交互式控制面语义留在共享运行时里，边界无法真正收口。

### 方案 B：把 question 彻底做成同步完成工具

未采用。因为这无法自然表达等待用户输入，也无法在恢复语义上保持干净。

### 方案 C：只记录 `question_requested/question_resolved`，不记录通用挂起事实

未采用。因为这样会让 runtime 无法拥有统一的挂起原语，后续其他挂起型工具仍会重新发明第二套机制。

## Rollout Status

当前已实现：

- `agent-core` 的共享 `Question*` 类型与 `PendingToolRequest`
- `builtin-tools::QuestionTool` 返回 `ToolCallOutcome::Suspended`
- `agent-runtime` 的通用 `Suspended` 结果、`tool_request_pending` / `turn_suspended` 事实和 `resume_turn_after_tool_result(...)`
- `apps/agent-server` 的 question 控制面、pending 恢复、回答/取消后续接原 turn

仍可继续完善：

- 把旧兼容事实 `turn_waiting_for_question` 自然退场
- 若未来出现第二类挂起型工具，验证当前通用原语无需再开 question 专名旁路

## Open Questions

1. 是否要为 `PendingToolRequest.kind` 建立更正式的共享枚举，而不是当前字符串协议
2. `question_requested` / `question_resolved` 是否需要独立 tape kind，而不继续复用 `event`
3. `dismissed`、`timed_out`、`unavailable` 的首个正式产品承接面应该先落 Web 还是先落 channel

## Success Criteria

当以下条件都成立时，可以认为当前边界设计是成功的：

- 支持交互式组件的 session 能向模型暴露 `Question`
- `QuestionTool` 通过共享工具协议挂起，而不是靠 runtime 特判
- runtime 能在不知道 question 语义的前提下挂起并恢复原 turn
- server 能稳定恢复 pending question 并驱动回答/取消闭环
- 旧 question 语义不再新增回 runtime 内部状态机
