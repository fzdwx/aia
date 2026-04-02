---
rfc: 0005
name: llm-automatic-retry
title: LLM Automatic Retry
description: Defines lower-layer automatic retry semantics for provider-backed LLM requests only, without extending retry to Web requests or general tool execution.
status: Draft
date: 2026-04-02
authors:
  - aia
supersedes: []
superseded_by: null
---

# RFC 0005: LLM Automatic Retry

## Summary

为 `aia` 引入 **仅限 LLM 调用链路** 的底层自动重试能力，核心结论是：

1. **只重试 LLM，不重试 Web 请求，不重试通用工具执行**。
2. **重试发生在 provider / adapter 调用层**，而不是前端 `fetch(...)` 层，也不是 `session_manager` 的 HTTP 路由层。
3. **只有在本次 LLM 请求还没有向上游 runtime 发出任何可见流式增量时，才允许自动重试**。
4. **一旦已经产生了任何 `StreamEvent` 文本 / thinking / tool-use 增量，就不得在底层透明重试**，避免重复内容、乱序工具调用和语义分叉。
5. **首版只覆盖 provider 级瞬时失败**，例如连接建立失败、早期超时、`429`、`5xx`、上游短暂不可用；不覆盖工具失败、tape 落盘失败或 session 控制面失败。

## Implementation Snapshot

截至当前代码，**本 RFC 尚未落地**。当前真实代码状态是：

1. `agent-runtime` 在 `drive_turn_loop(...)` 中每个 step 只调用一次 `model.complete_streaming(...)`，失败后直接转成 `RuntimeError::model(...)` 返回。
2. `openai-adapter` 当前没有统一 retry policy；请求发送和 SSE 读取阶段一旦失败，就直接返回 `OpenAiAdapterError`。
3. `agent-runtime` 当前只对“上下文过长”这类错误做一次 `compress_context(...)` 后重试；这属于语义级修复，不是通用自动重试。
4. `builtin-tools` 里的联网工具（如 `WebSearch` / `CodeSearch`）各自直接调 HTTP；它们不在本 RFC 覆盖范围内。

当前代码事实应优先对照：

- `crates/agent-runtime/src/runtime/turn/driver.rs`
- `crates/openai-adapter/src/streaming.rs`
- `crates/openai-adapter/src/http.rs`
- `crates/openai-adapter/src/{responses,chat_completions}/client.rs`
- `crates/agent-core/src/traits.rs`

## Motivation

当前最需要“自动重试”的，不是 Web 前端请求，也不是大多数工具调用，而是 **LLM provider 的瞬时失败窗口**。

### 1. 真实不稳定点主要在 provider I/O

当前 turn 主循环中，最脆弱的一段是：

1. runtime 组装 `CompletionRequest`
2. adapter 发出 provider 请求
3. 建立流式响应
4. 读取 SSE chunk

这段链路会遇到很多暂时性错误：

- 连接建立阶段超时
- 上游短时 `429`
- `502` / `503` / `504`
- provider 网关瞬断
- 在收到第一个有效事件前，流就被上游断开

这些失败很多都不代表“本次 turn 语义上失败了”，只是“这次尝试的传输窗口不好”。

### 2. 但 runtime 不能在任意阶段透明重试

`aia` 的 LLM 调用不是纯粹的“拿一段文本回来”这么简单。它是流式的，并且 completion 里可能包含：

- `thinking`
- `text`
- `tool_use`

一旦已经向上层发出了部分流式事件，再在底层重试同一个 LLM 请求，就会产生这些问题：

- 用户看到重复文本
- thinking 段前半段来自第一次请求，后半段来自第二次请求
- 第一次请求可能已经流出了工具调用意图，第二次请求又生成另一组工具调用
- trace 与 tape 很难保持单一、可解释的事实链

所以，**自动重试只能发生在“对上层还不可见”的阶段**。

### 3. 工具失败和 LLM 失败不是同一个问题

工具调用是有真实副作用和外部语义的：

- `Shell` 可能改文件
- `Write` / `Edit` / `ApplyPatch` 明显有副作用
- `Question` 依赖用户交互
- 联网工具是否重试，也要看具体供应商和速率限制策略

把“LLM provider 瞬时失败自动重试”直接泛化成“所有工具都自动重试”，风险会大很多，也不符合当前最急迫的问题边界。

## Goals

1. 为 LLM provider 调用引入统一、可配置、默认保守的自动重试语义。
2. 确保自动重试只发生在还没有向 runtime 上层发出任何可见 `StreamEvent` 的阶段。
3. 让 runtime 调用方不需要理解底层 reqwest / provider 的临时失败细节。
4. 保持 cancellation 语义优先，不能因为重试而吞掉取消。
5. 与现有的 `compress_context(...)` 语义兼容，不把“上下文压缩后再试”和“瞬时失败重试”混成一套机制。

## Non-Goals

1. 不为 Web 前端 `fetch(...)` 请求设计自动重试。
2. 不为 `builtin-tools` 的通用工具执行引入统一自动重试。
3. 不为 `Shell`、文件写入类工具、`Question`、tape / store 持久化增加自动重试。
4. 不在本 RFC 首版中引入跨 provider 的复杂熔断器、全局预算控制或分布式退避协调。
5. 不尝试在“已经向上游发出部分流式内容”后继续自动恢复同一个 completion。

## Proposal

### 核心原则：Retry Before First Visible Delta

底层自动重试只允许发生在 **LLM 请求已经发出，但 runtime 还没有收到并向上层发出任何可见增量** 的阶段。

这里的“可见增量”包括：

- `StreamEvent::TextDelta`
- `StreamEvent::ThinkingDelta`
- `StreamEvent::ToolCallDetected`
- `StreamEvent::ToolCallStarted`
- 以及任何未来会改变上层 turn 可见状态的流式事件

一旦某次尝试已经发出过任意可见增量：

- 本次尝试后续若失败，直接视为 turn 失败
- 不再进行透明自动重试

### 分层位置

自动重试逻辑应放在 **provider adapter / streaming 边界**，而不是：

- `apps/web`
- `apps/agent-server` HTTP 路由
- `session_manager`
- `ToolExecutor`

推荐落点：

- `crates/openai-adapter/src/streaming.rs`
- 或新增 `crates/openai-adapter/src/retry.rs`

这样可以保证：

1. retry 判定能直接看到 reqwest error / HTTP status / SSE 初始读取阶段错误
2. `agent-runtime` 仍然只面对单次 `complete_streaming(...)` 调用面
3. 不把 provider 特有重试策略污染回 `agent-core`

### Retry 适用范围

首版仅覆盖：

- `OpenAiResponsesModel::complete_streaming(...)`
- `OpenAiChatCompletionsModel::complete_streaming(...)`

换句话说，首版只覆盖：

- `crates/openai-adapter`

不覆盖：

- `builtin-tools` 里的 Exa / WebSearch / CodeSearch
- `channel-*` runtime 的网络调用
- `weixin-client` 之类的外部服务调用

### Retry 触发条件

首版默认允许自动重试的错误：

#### A. 请求发送前后、首包前的传输失败

- 连接建立超时
- DNS / TCP / TLS 建连失败
- reqwest transport error
- 上游在返回首个有效 SSE 事件前就断流

#### B. 明确的暂时性 HTTP 状态

- `408`
- `429`
- `500`
- `502`
- `503`
- `504`

#### C. 读取流时的“早期失败”

仅当：

- 还没有发出任何可见 `StreamEvent`
- 但读取 SSE body 时发生 error / early EOF / timeout

此时允许视为“对上层仍不可见”，可以自动重试。

### 明确不重试的错误

以下情况首版不自动重试：

- 已取消（`abort.is_aborted()` 或 adapter cancelled error）
- 请求模型与 adapter 配置不匹配
- `400` / `401` / `403` / `404` / `422`
- 内容策略 / schema / 参数错误
- 已经产出任意流式增量后的 mid-stream failure
- 工具执行失败
- tape 写入失败
- hook 失败
- runtime 内部 stop reason mismatch

### Retry Policy

首版建议使用非常保守的默认值：

- `max_attempts = 3`（含首次）
- `base_delay_ms = 300`
- `max_delay_ms = 2000`
- `jitter_ms = 150`

解释：

- 这里重试的是 LLM step，不是无副作用的纯读取接口
- delay 应短，优先覆盖瞬时 provider 抖动
- 不应把单个 turn 卡在超长退避里

### 新的 adapter 内部抽象

建议在 `openai-adapter` 内新增一层状态跟踪：

```rust
struct StreamingAttemptState {
    emitted_visible_event: bool,
}

impl StreamingAttemptState {
    fn mark_visible_event(&mut self) {
        self.emitted_visible_event = true;
    }

    fn can_retry(&self) -> bool {
        !self.emitted_visible_event
    }
}
```

然后 `complete_streaming_request(...)` 在把事件转发给上层 sink 前，先更新这个状态。

伪代码：

```rust
for attempt in 1..=max_attempts {
    let mut attempt_state = StreamingAttemptState::default();

    let result = run_single_streaming_attempt(..., |event| {
        if event.is_visible_delta() {
            attempt_state.mark_visible_event();
        }
        sink(event);
    }).await;

    match result {
        Ok(completion) => return Ok(completion),
        Err(error) if abort.is_aborted() || is_cancelled_error(&error) => {
            return Err(cancelled(error));
        }
        Err(error) if should_retry(&error) && attempt_state.can_retry() && attempt < max_attempts => {
            sleep(backoff_for(attempt)).await;
            continue;
        }
        Err(error) => return Err(error),
    }
}
```

### agent-runtime 与 adapter 的职责边界

`agent-runtime` 仍然保留当前主线：

- 继续只调用一次 `model.complete_streaming(...)`
- adapter 内部自己决定是否做早期重试

`agent-runtime` 只需要继续处理两类“更高层”的逻辑：

1. `compress_context(...)` 后重试
2. 普通模型失败转成 `RuntimeError::model(...)`

也就是说：

- **provider 瞬时失败重试** 在 adapter 内
- **上下文过长修复后重试** 在 runtime 内

两者语义不同，不应合并。

### 与 cancellation 的关系

cancellation 必须始终优先于 retry。

要求：

1. 每次 attempt 开始前检查 `abort`
2. backoff sleep 期间也必须响应 `abort`
3. 一旦判断为取消，不得进入下一次 attempt

否则会出现：

- 用户已经 stop/cancel
- provider 层还在默默 retry
- turn 看起来“停不下来”

### 与 trace / 可观测性的关系

首版至少需要把 attempt 信息纳入 trace / debug 上下文，避免排障时只看到“模型失败”，却看不到实际重试了几次。

建议最低要求：

- 在 adapter 日志 / trace 中记录 `attempt_index`
- 记录最终失败是否发生在“首个可见 delta 前”
- 记录触发 retry 的 HTTP status 或 transport error 摘要

首版不强制把每次 attempt 都暴露成独立的终端用户可见事件，但至少要有内部诊断信息。

## Alternatives Considered

### 方案 A：在 `agent-runtime` turn driver 层包一层整体 retry

不采纳。

原因：

- runtime 层太晚了，已经离 provider 细节太远
- 很难精确知道“这次失败时有没有已经发出过可见 delta”
- 容易把 provider retry 和 context compression retry 混在一起

### 方案 B：所有 mid-stream failure 都自动重连续传

不采纳。

原因：

- 现有 provider 协议没有可靠的通用 resume token 语义
- 一旦已经产生 partial text / tool-use，自动续传会让语义边界失控
- 这更像未来更重的“stream resume”问题，不是首版自动重试该做的事

### 方案 C：把联网工具也放进统一 retry 框架

暂不采纳。

原因：

- 工具失败语义差异大
- 有些工具明显有副作用
- 即使是 `WebSearch` / `CodeSearch` 这类无副作用联网工具，也应单独评估供应商限流、超时和结果一致性，而不是被 LLM retry 方案顺手裹进去

### 方案 D：完全不自动重试，所有失败都直接暴露

不采纳。

原因：

- provider 短时抖动在真实环境里很常见
- 很多失败发生在首个 delta 之前，对用户没有任何可见语义差别
- 这类失败完全可以在底层自动吸收

## Risks and Mitigations

### 1. 重试掩盖真实 provider 配置问题

风险：把本来就不会成功的错误重试几次，只会增加延迟。

缓解：

- 严格限制可重试错误集合
- `4xx` 配置 / 认证 / schema 类错误直接失败
- 只对明确瞬时失败进行 retry

### 2. 已经产生部分输出后仍被错误重试

风险：导致重复文本、重复工具调用、trace 混乱。

缓解：

- 在 adapter 内显式跟踪 `emitted_visible_event`
- 一旦为 `true`，后续失败直接返回，不再 retry

### 3. retry 让 stop/cancel 变慢

风险：用户已经取消，但底层还在 sleep/backoff。

缓解：

- backoff 必须 race `abort`
- 每次 attempt 前后都检查取消

### 4. 不同 provider 后续扩展时行为不一致

风险：未来若接更多 provider，retry 策略散落在各 adapter 内，难以统一。

缓解：

- 首版先在 `openai-adapter` 内收口为独立模块
- 后续若出现第二个 provider，再评估是否抽象出共享 retry helper

## Open Questions

1. “可见 delta” 的判定是否只看文本 / thinking / tool-call 事件，还是未来还要覆盖更多 event 类型？
2. 首版是否要把每次 attempt 作为独立 trace span 写进 `agent-store`？
3. 目前 `RequestTimeoutConfig` 只有 `read_timeout_ms`，是否需要补 `connect_timeout_ms` / `first_byte_timeout_ms`，让 retry 判定更精确？
4. `429` 是否应读取 provider 返回的 `Retry-After`，还是首版统一本地退避即可？

## Implementation Checklist

### 1. Retry 抽象与错误分类

- [ ] 在 `crates/openai-adapter/src/` 下新增独立 retry 模块，例如 `retry.rs`
- [ ] 定义 `RetryPolicy`，首版默认值为：`max_attempts=3`、`base_delay_ms=300`、`max_delay_ms=2000`、`jitter_ms=150`
- [ ] 定义 retryable 错误分类 helper，至少覆盖：transport error、`408`、`429`、`500`、`502`、`503`、`504`
- [ ] 明确排除不可重试错误：cancelled、`400`、`401`、`403`、`404`、`422`、模型配置不匹配、解析/协议错误
- [ ] 保持 `agent_core::LanguageModel` trait 不变，不把 retry 语义上提到 `agent-core`

### 2. Attempt 状态与可见 delta 判定

- [ ] 在 `openai-adapter` 内新增 `StreamingAttemptState`
- [ ] 显式记录 `emitted_visible_event: bool`
- [ ] 在事件转发到 sink 前标记可见事件
- [ ] 首版把这些事件视为“可见 delta”：文本增量、thinking 增量、tool call 检测/开始事件
- [ ] 一旦 `emitted_visible_event=true`，当前 attempt 后续失败必须直接返回，不再 retry

### 3. Responses / Chat Completions 接入 retry loop

- [ ] 将 `complete_streaming_request(...)` 拆成“单次 attempt”与“attempt loop”两个层次
- [ ] send 阶段失败时按 retry policy 重试
- [ ] HTTP status 非成功且属于 retryable 时按 retry policy 重试
- [ ] SSE 读取阶段如果在首个可见 delta 前失败，按 retry policy 重试
- [ ] SSE 读取阶段如果在首个可见 delta 后失败，直接返回错误
- [ ] `OpenAiResponsesModel::complete_streaming(...)` 接入新逻辑
- [ ] `OpenAiChatCompletionsModel::complete_streaming(...)` 接入新逻辑

### 4. Cancellation 优先级

- [ ] 每次 attempt 开始前检查 `AbortSignal`
- [ ] backoff sleep 期间使用可中断等待，不能无视 `abort`
- [ ] send 阶段收到 cancelled 时直接返回，不进入下一次 attempt
- [ ] stream 读取阶段收到 cancelled 时直接返回，不进入下一次 attempt
- [ ] 为 cancel 场景补测试，确认 retry 不会拖慢 stop/cancel

### 5. Runtime 边界保持清晰

- [ ] 保持 `agent-runtime` 继续只调用一次 `model.complete_streaming(...)`
- [ ] 不把 provider retry 和 `compress_context(...)` 后重试合并到一处
- [ ] 保持当前 `RuntimeError::model(...)` 映射路径不变，只让 adapter 内部先做吸收
- [ ] 不改 `ToolExecutor`、`session_manager`、`apps/web` 的现有调用面

### 6. 可观测性

- [ ] 在 adapter trace / log 中记录 `attempt_index`
- [ ] 记录最终失败是否发生在首个可见 delta 前
- [ ] 记录触发 retry 的 error 摘要或 HTTP status
- [ ] 评估是否需要把 attempt 信息挂入现有 trace span，而不是新增用户可见事件

### 7. 测试

- [ ] 补单测：首包前 `503` 后重试成功
- [ ] 补单测：首包前 transport error 后重试成功
- [ ] 补单测：首包前 early EOF 后重试成功
- [ ] 补单测：已发出 text delta 后断流，不再重试
- [ ] 补单测：已发出 tool-use 相关可见事件后失败，不再重试
- [ ] 补单测：cancel 发生在 backoff 期间，不再继续 retry
- [ ] 补单测：不可重试的 `4xx` 直接失败

### 8. 验收与后续决策

- [ ] 验证真实 provider 首包前瞬时失败场景下 turn 成功率是否提升
- [ ] 验证不会出现重复文本、重复 thinking、重复 tool-use
- [ ] 验证 stop/cancel 延迟没有明显恶化
- [ ] 决定是否把相同策略扩展到未来其他 provider adapter
- [ ] 明确首版仍不扩展到 `builtin-tools` 联网工具层

## Success Criteria

1. 对于 provider 首包前的短暂 `429` / `5xx` / transport failure，turn 成功率明显提升。
2. 已经发出部分流式内容后发生的失败，不会被透明重试成重复输出。
3. `cancel` / `interrupt` 仍然保持及时生效，不会被 retry 延迟吞掉。
4. `agent-runtime` 外部调用面不需要感知 provider retry 细节。
5. 首版落地后，`Web` 请求层和工具执行层行为保持不变，没有被顺手扩大范围。
