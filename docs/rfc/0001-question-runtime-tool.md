---
rfc: 0001
name: question-runtime-tool
title: Question Runtime Tool
description: Defines the internal Question runtime tool, capability gating, structured results, and recoverable pending-question semantics.
status: Accepted
date: 2026-03-25
authors:
  - aia
supersedes: []
superseded_by: null
---

# RFC 0001: `Question` Runtime Tool

## Summary

为 `aia` 引入内部 `Question` runtime tool：仅在当前 session 支持交互式组件时向模型暴露该工具；问题请求与结果均使用结构化 JSON；运行时支持暂停等待用户回答，并可通过 append-only session tape 在停机后恢复 pending question。

## Motivation

当前 `aia` 已经具备统一工具协议、append-only session tape、server control-plane 和多种 channel/runtime 承接能力，但“执行中向用户发起澄清问题并等待回答”这条能力仍未形成正式协议。

仓库现状里，Web 已经预留了 `question` 工具的时间线渲染入口，但 runtime、server、session 恢复与 channel capability 还没有对应的完整语义。这导致代理在遇到“有多个合理路径、但不该擅自猜测”的场景时，只能：

- 直接猜一个默认答案继续做
- 输出一段文本要求用户回复，但 runtime 不知道自己其实应该暂停等待
- 在不同承接面里各自实现一套临时交互逻辑

这和 `aia` 当前的核心方向不一致：

- server 应作为 canonical runtime control surface
- session tape 必须 append-only，可恢复，不依赖派生态偷偷替换事实
- 内部协议应稳定、统一，不为外部模型家族差异而污染核心层

## Goals

本 RFC 只定义 `aia` 内部的 `Question` runtime tool 及其运行时语义，不涉及任何外部协议适配。

具体目标：

1. 允许模型在 turn 执行中发起结构化问题，并暂停等待用户回答
2. 让 `Question` 是否可用由 session / channel capability 决定
3. 不支持交互式组件的 session 不注册 `Question` tool
4. 问题与答案都以结构化 JSON 表达，而不是只回一段自由文本
5. 支持 AI 为选项给出“推荐项 + 推荐理由”
6. 支持 server 停机、重启后从 session tape 恢复“仍在等待回答”的状态
7. 保持 append-only tape 语义，不通过覆写 session 派生状态替代源事实

## Non-Goals

本 RFC 不包含以下内容：

- 不做 Claude / Gemini / MCP 等外部协议适配
- 不为不支持交互式组件的 channel 提供“文本降级版 question tool”
- 不在本轮引入子代理中的 question 语义
- 不设计复杂审批流、多人会签或 channel 级投票
- 不尝试让模型自动替用户选择推荐项

## Proposal

### 0. 权威状态来源

`Question` 的可用性与交互承接能力，不由 runtime 或具体 channel 在执行时临时推断，而由 server 在 session 创建、恢复或切换承接面时产出一份稳定的 session 级能力快照。

建议新增共享结构：

```json
{
  "supports_interactive_components": true,
  "supports_question_tool": true
}
```

建议命名为 `SessionInteractionCapabilities`，并遵循以下职责分工：

- `apps/agent-server`：负责根据当前承接面（如 Web、channel profile、未来桌面壳）计算并维护该能力快照
- `agent-runtime`：只读取这份能力快照决定是否向模型暴露 `Question`
- `apps/web` / 其他客户端：只消费这份能力快照用于 UI 与交互呈现，不各自再发明第二套判断逻辑

其中：

- `supports_interactive_components` 表示当前 session 是否存在可承接 modal / form / structured prompt 的交互面
- `supports_question_tool` 表示 runtime 是否应把 `Question` 注册到当前 session 的 visible tools

在当前阶段，推荐保持两者同值；保留两个字段只是为了给未来更细粒度能力拆分留出口。

### 1. `Question` 是 runtime tool，不是普通 builtin tool

`Question` 的本质不是“执行一个同步函数并立即返回结果”，而是：

- 生成问题
- 把问题交给当前 session 的交互承接面
- 暂停当前 turn
- 等待用户回答
- 恢复原 turn 继续执行

因此它应落在 runtime tool 这一层，而不是 `builtin-tools`。

这样可以保持职责清晰：

- `agent-core`：承接共享类型和结构化协议
- `agent-runtime`：承接暂停 / 恢复 / tool result 注入语义
- `apps/agent-server`：承接 pending question 控制面与 session 状态恢复
- `apps/web`：承接交互 UI

### 2. `Question` 的可见性由 session interaction capability 决定

不是所有 session 都应该向模型暴露 `Question`。

本 RFC 采用 capability gating：只有当前 session 明确支持交互式组件时，runtime 才把 `Question` 注册到模型可见工具列表中。

建议新增 session 级 capability，例如：

- `supports_interactive_components: bool`
- 或更直接地命名为 `supports_question_tool: bool`

推荐使用更通用的前者，再由 runtime 映射到是否注册 `Question`。

这样可以覆盖两类来源：

- Web UI 这类天然支持 modal / form 的承接面
- 某些 channel 虽然存在，但当前 profile 或当前入口不支持交互式组件

若 capability 为 `false`：

- `Question` 不加入 visible tools
- 模型无法主动调用它
- server / UI 也不需要为该 session 准备 pending question 交互面

同时，建议明确以下约束：

- 同一 session 任意时刻最多只能存在一个 pending `QuestionRequest`
- `Question` 是否注册只由 `SessionInteractionCapabilities` 决定，不允许 runtime 再根据 tool call 现场兜底猜测

### 3. 结果必须对模型结构化，而不是只返回一句话

`Question` 的 tool result 不应只返回类似：

- `User answered your question: ...`
- `Question ignored`
- `No answer provided`

因为这会让模型分不清以下几种完全不同的情况：

- 用户明确做出了回答
- 用户点击取消
- UI 关闭了但未提交
- session 当前不支持交互式问题
- 系统没有成功采集到答案
- 超时

因此 `Question` 的结果必须是结构化 JSON，并在 `ToolResult.content` 与 `ToolResult.details` 中保持同构表达。

## 协议草案

### `QuestionRequest`

```json
{
  "request_id": "qreq_123",
  "invocation_id": "call_123",
  "turn_id": "turn_123",
  "questions": [
    {
      "id": "database",
      "header": "Database",
      "question": "要使用哪个数据库？",
      "kind": "choice",
      "required": true,
      "multi_select": false,
      "options": [
        {
          "id": "postgres",
          "label": "PostgreSQL",
          "description": "更适合未来扩展和并发场景"
        },
        {
          "id": "sqlite",
          "label": "SQLite",
          "description": "部署最简单，适合单机场景"
        }
      ],
      "recommended_option_ids": ["sqlite"],
      "recommendation_reason": "当前项目是单机 agent harness，本地部署与测试成本最低"
    }
  ]
}
```

字段建议：

- `request_id`: 一次 question 请求的稳定标识，用于恢复与答复提交，也是 pending question 的唯一主键
- `invocation_id`: 对应的 tool invocation id
- `turn_id`: 当前 turn
- `questions[]`: 1..N 个问题

每个问题建议包含：

- `id`: 问题 ID，供 answer 精确回填
- `header`: 短标题，供 UI badge / tab / section 使用
- `question`: 完整问题文本
- `kind`: `choice | text | confirm`
- `required`: 是否必须回答
- `multi_select`: 多选开关，仅 `choice` 使用
- `options[]`: 结构化候选项
- `placeholder`: `text` 类型占位提示，可选
- `recommended_option_ids[]`: AI 推荐项，可为空
- `recommendation_reason`: 推荐理由，可为空

字段约束建议：

- `choice`：必须包含 `options[]`
- `text`：默认不包含 `options[]`
- `confirm`：Phase 1 统一视为受限的 `choice` 变体，允许实现侧固定为 `yes/no` 两项，不额外引入独立布尔 wire shape
- `header` 应保持简短，适合 UI badge / tab / chip 展示
- `recommended_option_ids[]` 必须是 `options[]` 中已存在的 option id 子集

### `QuestionOption`

```json
{
  "id": "sqlite",
  "label": "SQLite",
  "description": "部署最简单，适合单机场景"
}
```

约束：

- `id` 应稳定、机器可读
- `label` 给用户展示
- `description` 用于解释差异

### `QuestionAnswer`

```json
{
  "question_id": "database",
  "selected_option_ids": ["sqlite"],
  "text": null
}
```

说明：

- `choice`：主要依赖 `selected_option_ids`
- `text`：主要依赖 `text`
- `confirm`：可视为单选 `yes/no` 或单个布尔语义选项

### `QuestionResult`

```json
{
  "status": "answered",
  "request_id": "qreq_123",
  "answers": [
    {
      "question_id": "database",
      "selected_option_ids": ["sqlite"],
      "text": null
    }
  ]
}
```

状态建议枚举：

- `answered`
- `cancelled`
- `dismissed`
- `timed_out`
- `unavailable`

`unavailable` 示例：

```json
{
  "status": "unavailable",
  "request_id": "qreq_123",
  "reason": "interactive question tool is not supported in this session"
}
```

`cancelled` 示例：

```json
{
  "status": "cancelled",
  "request_id": "qreq_123",
  "answers": []
}
```

补充约束：

- `request_id` 必须精确匹配当前 pending request 的唯一主键
- `answered` 是唯一允许携带用户答案的成功完成态
- `cancelled`、`dismissed`、`timed_out`、`unavailable` 不应伪装成“已有用户确认”
- `QuestionResult.content` 推荐使用紧凑 JSON；`ToolResult.details` 保留同构结构化对象，避免为 prettified 文本浪费模型上下文 token

## 运行时语义

### 状态机

建议为 session 增加显式 question 等待态：

- `Idle`
- `Running`
- `WaitingForQuestion`

基本流转：

1. session 进入 `Running`
2. 模型调用 `Question`
3. runtime 生成 `QuestionRequest`
4. session 切到 `WaitingForQuestion`
5. server/UI 向用户展示问题
6. 用户提交 `QuestionResult`
7. runtime 收到结果后恢复原 turn
8. session 回到 `Running`
9. turn 完成后回到 `Idle`

### 为什么不把它当普通 tool 同步返回

如果同步返回，就只能：

- 伪造一个立即结果
- 或在 tool 内部阻塞等待 UI

前者会让模型误以为已经拿到用户反馈；后者会把 server 交互、停机恢复、session 状态都藏进临时内存路径里，不符合当前架构方向。

因此正确做法是：

- runtime 把 `Question` 视为可挂起的 runtime tool
- 当前 turn 暂停在 pending question 上
- 恢复后把结构化 `QuestionResult` 注入回工具结果链路

## Session Tape 设计

### 原则

`Question` 必须遵守 append-only tape 语义。

不允许：

- 在用户回答后覆写原问题
- 只靠内存态记住“当前有个 pending question”
- server 重启后因没有持久化事实而丢失恢复能力

### 建议记录的事实

至少记录两类事实：

1. `question_requested`
2. `question_resolved`

同时保留正常的：

- `tool_call`（调用 `Question`）
- `tool_result`（最终的结构化 `QuestionResult`）

### 推荐落盘顺序

当模型调用 `Question` 时：

1. 追加 `tool_call(question)`
2. 追加 `event(question_requested)`

当用户回答或取消时：

3. 追加 `event(question_resolved)`
4. 追加 `tool_result(question)`

这样做的意义：

- `tool_call` / `tool_result` 保持统一工具调用语义
- `question_requested` / `question_resolved` 额外为 server 恢复与 UI 投影提供稳定事实
- 即使未来要在 trace、dashboard、session info 里单独观察“用户澄清交互”，也有明确事件可投影

### 恢复逻辑

server 启动或 session hydrate 时：

- 如果发现最新的 `question_requested` 没有匹配的 `question_resolved`
- 则把 session 恢复为 `WaitingForQuestion`
- 并恢复对应的 `QuestionRequest`

这里的“匹配”建议明确为：

- `question_requested.request_id == question_resolved.request_id`

不应依赖：

- `turn_id` 模糊匹配
- 最近一条事件位置推断
- tool 名称或 question 文本内容推断

恢复时应始终以 `request_id` 作为唯一关联键。

这样可覆盖：

- server 正常重启
- 进程崩溃
- Web 刷新
- 其他客户端稍后重新附着到同一 session

## Server 控制面建议

本 RFC 将 server control-plane 进一步收口为一组明确接口，而不是只保留建议级轮廓。

建议固定为：

- `GET /api/session/question?session_id=...`
- `PUT /api/session/question`
- `DELETE /api/session/question?session_id=...`

语义如下：

### `GET /api/session/question`

用途：

- 读取当前 session 是否存在 pending question
- 若存在，返回完整 `QuestionRequest`
- 若不存在，返回空状态而不是错误

推荐响应形状：

```json
{
  "pending": true,
  "request": {
    "request_id": "qreq_123",
    "invocation_id": "call_123",
    "turn_id": "turn_123",
    "questions": []
  }
}
```

### `PUT /api/session/question`

用途：

- 向当前 pending request 提交结构化 `QuestionResult`

约束：

- 请求体直接使用 `QuestionResult`
- `request_id` 必须匹配当前 pending question
- 若当前不存在 pending question，应返回冲突或显式 bad request

### `DELETE /api/session/question`

用途：

- 显式取消当前 pending question

推荐语义：

- server 内部将其归一化为 `status = cancelled` 的 `QuestionResult`
- 仍然走 `question_resolved + tool_result(question)` 的统一落盘路径

额外约束：

- 同一 session 同时最多只允许一个 pending question，因此不需要在 control-plane 再引入第二层 question 集合资源
- 当 session 处于 `WaitingForQuestion` 时，不接受新的普通 `POST /api/turn` 提交；此时只允许三类交互：读取当前 pending question、提交当前 question 的回答、取消当前 question

## UI / Channel 行为

### Web

Web 应作为首个正式承接面：

- session 进入 `WaitingForQuestion` 时显示 modal / drawer / inline composer
- 明确展示推荐项和推荐理由
- 用户提交后把结构化结果发回 server
- timeline 继续显示这次 `Question` 调用的历史记录

现有 `question` renderer 可继续承担“历史 / 已完成工具结果”的展示，不承担 pending 交互本身。

### Channel

本 RFC 明确要求：

- 不支持交互式组件的 channel / session 不注册 `Question`
- 不在本轮提供文本降级路径

这样可以避免把“真正的交互式澄清”能力做成半吊子的伪交互。

若未来要支持 channel 文本降级，应另起 RFC，单独定义：

- 文本提问格式
- 如何从后续消息识别这是回答而不是新 turn
- 如何处理多题、多选、取消、超时

## AI 推荐项

`Question` 允许模型在生成选项时同时生成：

- `recommended_option_ids[]`
- `recommendation_reason`

约束：

- 推荐只用于帮助用户快速理解取舍
- 不允许 runtime 或 UI 自动替用户选择推荐项
- 用户显式选择始终高于推荐

为什么要有这一层：

- 它保留了 agent 的判断力
- 又不会把决定权偷偷拿走
- 在实现/架构/依赖取舍问题上，能显著减少用户理解成本

## Risks and Mitigations

本提案的主要风险与缓解方式如下。

### 1. 模型把“没有真实回答”误判成“用户已确认”

缓解方式：

- `QuestionResult` 强制包含结构化 `status`
- 不允许仅靠自然语言 tool result 表达结果
- 明确区分 `answered/cancelled/dismissed/timed_out/unavailable`

### 2. session 重启后丢失 pending question

缓解方式：

- 在 tape 中追加 `question_requested` / `question_resolved`
- 恢复时依据这两类事实重建 `WaitingForQuestion`

### 3. 把不可用能力暴露给模型

缓解方式：

- 由 session capability 决定是否注册 `Question`
- 不支持交互式组件的 session 根本不向模型暴露该工具

### 4. 推荐项被错误当成自动决策

缓解方式：

- 推荐仅用于 UI 展示和用户参考
- runtime 与 UI 都不自动代替用户提交推荐项

### 5. question 生命周期与普通 turn API 并发冲突

缓解方式：

- 显式约束同一 session 同时最多只有一个 pending question
- `WaitingForQuestion` 期间拒绝新的普通 turn 提交，但不把“查看 / 回答 / 取消当前 question”误判为被禁用的用户交互
- `PUT /api/session/question` 以 `request_id` 做幂等和冲突判断

## 错误与边界条件

### 1. 当前 session 不支持 question

- `Question` 不注册
- 模型不可见
- 不应依赖运行时再返回 `unavailable`

`unavailable` 结果更多用于：

- 恢复过程中状态失配
- control-plane 中途不可达
- UI 承接面声明支持但当前实际不可用

### 2. 重复提交 answer

- 只接受第一个能成功匹配 pending request 的回答
- 后续重复提交返回冲突或幂等成功
- 不允许同一 request 产生多个最终 `question_resolved`

### 3. Session 正在等待 question 时收到新 turn

建议直接拒绝新的普通 turn 提交，并返回显式错误，例如：

- `session is waiting for a question response`

这样比偷偷把新消息当问题回答或排队更可控。

这里的“拒绝新 turn”含义应明确为：

- 不接受新的普通任务消息去开启另一个 agent turn
- 不等于禁止所有用户交互
- 用户仍然应被允许查看当前问题、提交回答或取消该问题

Web 等承接面应尽量把这层语义显式投影到交互上，例如：

- 输入区提示“请先回答当前问题或取消该问题”
- 若用户尝试发送普通消息，前端直接拦截并提示，而不是把它提交成新的 `POST /api/turn`

### 3.1 Session capability 在恢复后变化

可能出现：

- session 先前通过支持交互组件的承接面发起了 question
- server 重启后只剩不支持交互组件的承接面在线

处理建议：

- 仍然恢复 pending question 事实
- 但不重新向模型暴露新的 `Question` tool
- control-plane 应继续允许已有 pending question 被查看、回答或取消
- 若当前承接面无法完成回答，则通过显式 UI / API 提示处理，而不是静默丢弃 pending 状态

### 4. tool result 空答案歧义

不允许把“空 answers”直接等价为成功回答。

必须依赖 `status` 判定：

- `answered` + `answers=[]`：只在明确允许空回答的问题下才成立
- `cancelled`：用户主动取消
- `dismissed`：UI 关闭
- `timed_out`：超过等待时间
- `unavailable`：系统未成功完成承接

## Rollout Plan

### Phase 1：共享协议与恢复语义

- `agent-core`：新增 `QuestionRequest`、`QuestionItem`、`QuestionOption`、`QuestionAnswer`、`QuestionResult`
- `agent-runtime`：新增 `Question` runtime tool、`SessionInteractionCapabilities` 消费与 suspend/resume 语义
- `session-tape` / `agent-server`：补 `question_requested` / `question_resolved` 的持久化与恢复

当前进度：

- 已完成：`agent-core` 的共享 `Question*` 类型、`SessionInteractionCapabilities`、`agent-prompts` 的 `question` 工具描述、`agent-runtime` 的 `Question` runtime tool 定义与 capability gating
- 未完成：`session-tape` / `agent-server` 的 pending question 事实落盘、恢复路径与 suspend/resume 主链

### Phase 2：server control-plane

- `apps/agent-server`：新增 `GET/PUT/DELETE /api/session/question` 控制面
- `SessionSlot` / session state：补 `WaitingForQuestion`

### Phase 3：Web 承接

- `apps/web`：新增 pending question UI
- 继续复用现有时间线里的 `question` renderer 展示历史结果

### Phase 4：超时语义（可选，非首轮必做）

- Phase 1-3 不强制实现自动超时
- `timed_out` 先保留为协议枚举和未来扩展位
- 若后续要启用自动超时，应另行明确默认超时、回收任务、UI 提示与恢复语义

## Alternatives Considered

### 方案 A：把 `Question` 做成普通 builtin tool

不采纳。原因：

- 无法自然表达暂停 / 恢复
- 停机恢复会很别扭
- server control-plane 会被迫绕过 runtime 语义

### 方案 B：始终注册 `Question`，不支持的 session 再返回失败

不采纳。原因：

- 模型会被暴露一个实际上不可用的能力
- 增加无意义失败路径
- 不符合 capability gating 的设计方向

### 方案 C：不做结构化结果，只给模型一段文本

不采纳。原因：

- 无法区分 answered / cancelled / unavailable / timed_out
- 模型容易误解，进而在没有真实用户确认的情况下继续执行

### 方案 D：不落 tape，只保存在 server 内存

不采纳。原因：

- server 重启即丢状态
- 不满足 append-only 和恢复要求

## Open Questions

1. `Question` 是否应该支持一次调用多个问题，还是 Phase 1 先限制为单题更稳
2. `question_requested` / `question_resolved` 应落在 `event` 还是更专门的 tape kind 上
3. 是否要为 `QuestionRequest` / `QuestionResult` 额外补一层共享序列化版本号字段，便于未来协议演进

## Success Criteria

当以下能力都成立时，可认为 `Question` runtime tool 基本落地：

- 支持交互式组件的 session 能向模型暴露 `Question`
- 模型可发起结构化问题并暂停 turn
- 用户可以从 control-plane / Web 回答问题
- runtime 可恢复原 turn 并拿到结构化 `QuestionResult`
- 停机重启后仍能恢复 pending question
- 不支持交互式组件的 session 不会暴露 `Question`
