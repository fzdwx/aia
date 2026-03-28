---
rfc: 0002
name: session-auto-rename
title: Session Auto Rename
description: Defines AI-driven session renaming, rename cadence, session update events, and frontend title animation/last-active projection semantics.
status: Implemented
date: 2026-03-28
authors:
  - aia
supersedes: []
superseded_by: null
---

# RFC 0002: `Session` 自动重命名

## Summary

为 `aia` 引入统一的 session 自动重命名能力：新 session 仍以默认标题创建，但 session manager 会在每 `3~5` 次用户对话后，基于最近一段对话上下文触发一次低优先级 AI 重命名；该能力以 session 级标题来源状态为约束，只在 `default` / `auto` 标题上运行，不覆盖用户手动命名或外部 channel 已给出的标题，并通过统一 `session_updated` 事件同步到 Web 等客户端。前端在收到标题更新时，对 session 名称使用轻量打字机效果展示，并在列表中同时展示 `最近活跃时间`。

## Motivation

当前 session 创建路径会直接写入默认标题 `New session`。这个行为本身没有问题，它能保证 session 在创建瞬间就具备稳定 identity，也避免 UI、store 与外部 channel 在“session 先创建还是标题先生成”之间互相等待。

但只保留默认标题也有明显问题：

- session 列表很快会被大量 `New session` 淹没
- 用户只能依赖时间顺序或 session id 回忆上下文
- 外部 channel 触发的新对话更容易失去主题可读性
- 当前系统无法区分“默认标题”“自动标题”“用户手改标题”，后续若直接追加自动改名逻辑，很容易把用户自定义标题覆盖掉

与此同时，现有代码已经具备一个很合适的收口点：

- session 创建统一经由 `apps/agent-server` 的 session manager
- turn 生命周期在 server 侧有稳定的完成态
- session 标题已进入 SQLite store，可作为跨重启、跨承接面的 canonical state
- Web session 列表通过 store / SSE 投影消费 session 元信息

因此，这个能力最适合做成 server 主导的共享 session 语义，而不是让 Web 临时猜、让 channel 各自实现，或者让模型每次都额外生成一遍标题。

## Goals

- 为新 session 提供自动生成的可读标题，减少默认标题堆积
- 保持 session 创建路径同步、稳定、低延迟，不为标题生成阻塞主链
- 让自动重命名同时覆盖 Web、新 channel 会话、self-chat 等所有 session 来源
- 允许特定 channel 或 profile 明确声明“不参与自动重命名”
- 显式区分标题来源，防止自动重命名覆盖手动命名结果
- 保持自动重命名可重复判定、可恢复、可审计，而不是散落在前端局部状态里
- 让自动重命名使用 AI 生成标题，而不是只做本地截断规则
- 让重命名节奏保持克制，避免每轮都改标题带来抖动
- 让客户端通过统一 session 更新事件感知标题与活跃时间变化，而不是依赖整表轮询刷新
- 为前端标题变化提供轻量但明确的动画反馈，不影响列表稳定性
- 在 session 列表中直接体现最近活跃时间，提升可扫描性

## Non-Goals

- 本 RFC 不设计完整的 session 手动重命名 UI
- 本 RFC 不试图对历史全部 session 做一次性回填重命名
- 本 RFC 不把标题生成做成 provider / model 特性
- 本 RFC 不引入第二套只给某个客户端使用的 session 命名机制
- 本 RFC 不要求前端为最近活跃时间做复杂分组视图，例如“今天 / 昨天 / 本周”分段
- 本 RFC 不要求打字机动画跨刷新恢复；该动画只是一种临时表现层效果

## Proposal

### 1. 标题权威来源仍然是 store 中的 session record

session 标题的权威状态继续保留在 `agent-store` 的 `sessions` 表中，而不是转移到：

- Web 本地 store
- session tape 的派生投影
- runtime 内存态
- channel 专属 profile / binding

原因很简单：session 标题属于 session 元信息，天然需要跨重启、跨客户端、跨入口保持一致。SQLite store 已经承担这类职责，继续沿用这条边界最稳。

### 1.1 自动重命名资格还受 session 承接策略约束

虽然标题权威状态在 store，但“这个 session 能不能自动重命名”不应只靠标题来源决定，还应受 session 的承接策略约束。

本 RFC 建议引入 session 级的自动重命名资格概念，例如：

- `auto_rename_policy`

建议枚举值：

- `enabled`
- `disabled`
- `inherit`

其中：

- `enabled`：明确允许当前 session 参与自动重命名
- `disabled`：明确禁止当前 session 参与自动重命名
- `inherit`：由当前 session 的创建来源或 channel/profile 默认策略决定

这条策略与 `title_source` 是正交的：

- `title_source` 解决“当前标题能不能被覆盖”
- `auto_rename_policy` 解决“当前 session 是否允许进入自动重命名系统”

两者都满足时，session 才真正具备自动重命名资格。

### 2. 为 session 标题补充显式来源字段

当前仅有 `title` 字符串，不足以表达自动重命名需要的保护语义。

本 RFC 引入新的 session 级字段：

- `title_source`

建议枚举值：

- `default`
- `auto`
- `manual`
- `channel`

语义如下：

- `default`：系统刚创建 session 时写入的默认标题
- `auto`：系统基于自动重命名策略生成的标题
- `manual`：用户或上层显式手动设置的标题
- `channel`：session 创建时直接继承自外部 channel 会话标题、群名、对话主题等上游命名

自动重命名只允许在以下来源上覆盖：

- `default`
- `auto`

自动重命名禁止覆盖以下来源：

- `manual`
- `channel`

这样可以解决当前最大的根因问题：系统终于能区分“自己能不能继续改这个标题”。

### 2.1 为 session 补充最近活跃时间字段

除了标题来源，本 RFC 还要求 session 元信息显式暴露最近活跃时间：

- `last_active_at`

建议语义：

- 创建 session 时，`last_active_at` 初始化为 `created_at`
- 每次用户 turn 被接受进入 session 后，更新 `last_active_at`
- 若外部 channel 自动触发 turn，也同样更新 `last_active_at`

该字段与 `updated_at` 不应完全等价。

推荐区别如下：

- `updated_at`：session record 任意元信息被更新时变化，例如标题变化、模型变化、其他元数据更新
- `last_active_at`：只有真实对话活跃发生时变化，更适合作为列表里的“最近活跃时间”

这样能避免“只是自动重命名了一次标题，就把 session 伪装成刚刚活跃过”。

### 3. 创建 session 时仍然先写默认标题

session 创建路径不改为“先等标题生成，再落库”，仍保持：

1. 生成 session id
2. 写入 `title = New session`
3. 写入 `title_source = default`
4. 返回 session record

这是一个刻意保留的设计约束：

- session 创建必须同步完成
- 不增加首包等待
- 不把标题生成失败传播为 session 创建失败
- 不让前端/外部 channel 因为等标题而卡住 session 生命周期

若某些入口在创建时已经拥有稳定标题，也允许显式覆盖默认值：

- 传入自定义标题时，直接写该标题
- 同时按来源写 `manual` 或 `channel`

同时，session 创建时还应确定自动重命名资格：

- 普通 Web / self-chat 创建的 session，默认 `auto_rename_policy = enabled`
- 由 channel 创建的 session，默认取该 channel/profile 的自动重命名策略

但“无标题创建后自动生成”仍是主路径。

### 3.1 特定 channel 可明确禁用自动重命名

并不是所有 channel 都适合自动重命名。

例如某些场景里：

- channel 已经天然有稳定对话主题或群名
- channel 会话标题来自业务侧主数据，不应被 AI 改写
- channel 本身是低延迟、高频消息流，不值得再追加标题模型调用
- 某些 profile 对隐私、成本或产品语义有更强约束

因此，本 RFC 明确要求：

- channel adapter 或其 profile 配置可以声明“该入口创建的 session 不参与自动重命名”

推荐优先级如下：

1. session 显式 `auto_rename_policy`
2. channel profile 配置
3. channel transport 默认值
4. 全局默认值

建议默认行为：

- Web：启用
- self-chat：启用
- channel：默认禁用，除非该 channel/profile 显式开启

这条默认值是保守的，因为 channel 往往更容易带入上游命名、外部业务语义和额外成本约束。

### 4. 自动重命名采用低优先级 AI 生成，而不是纯规则截断

本 RFC 采纳 AI 驱动的 session 自动重命名，而不是只做基于首条消息的本地清洗规则。

推荐原因：

- 标题质量通常显著更高
- 可以综合多轮上下文，而不是机械截断单条用户输入
- 对真实编码/问答 session 更容易抽到正确主题

但为了避免 AI 重命名反向污染主链，本 RFC 约束它必须满足以下条件：

- 作为 turn 完成后的异步 side effect 触发
- 失败不影响本轮 turn 成功与否
- 默认走当前 session 已绑定的 model/provider，除非后续明确抽出专用标题模型
- 请求体尽量小，只带重命名真正所需的最近对话摘要输入
- 输出必须是极短标题，而不是完整句子或解释

建议的标题生成输入来源：

- 最近 `3~5` 个用户 turn 的 `user_message`
- 必要时附带对应轮的 assistant 简要回复或压缩后的主题线索
- 不要求带完整 tool 输出，除非后续验证证明对标题质量有实质帮助

AI 输出目标：

- 可读
- 足够短
- 能反映主题
- 不强求语言学完美

建议长度目标：

- 标题生成当前主要依赖共享 title prompt 约束与最基本的服务端空值过滤；更激进的本地规范化规则没有进入最终实现，以避免在 server 侧再维护一套与 prompt 重叠的标题裁剪语义。

### 4.1 AI 标题生成契约

建议新增一个 server 内部使用的重命名任务边界，例如：

- `SessionRenamePlanner`
- `SessionRenameService`
- 或更明确的 `SessionAutoRenameService`

其输入建议为：

```json
{
  "session_id": "20260328_abcd1234",
  "current_title": "New session",
  "title_source": "default",
  "recent_user_turns": [
    "现在我们开始来设计一个session的自动重命名功能",
    "编写一个rfc",
    "继续把 RFC 收紧成更工程化的实现稿"
  ]
}
```

其输出建议为：

```json
{
  "title": "设计 Session 自动重命名 RFC"
}
```

约束建议：

- 输出只允许一个短标题字段
- 不允许解释、编号、markdown 列表
- 不允许返回空字符串
- 规范化后若标题无效，则视为本次跳过

### 4.2 AI 标题生成请求的 trace 归属

AI 重命名会引入额外模型调用，因此需要在 trace 里与普通对话区分。

建议新增独立 request kind：

- `session_rename`

这样：

- trace overview / dashboard 可单独观察这类请求量
- 出现重命名风暴时能直接定位
- 后续若要限流、降级、关闭，也有明确可观测边界

### 5. 触发时机放在 turn 完成后的 server 收口点

自动重命名的推荐触发时机是：

- turn 已完成并产出稳定 `TurnLifecycle`
- session manager 已经拿到该 turn 的可持久化结果

不推荐以下时机：

- session 创建瞬间：那时还没有真实对话主题
- 流式进行中：标题会抖动，且 turn 可能失败/取消
- 只在 Web 端收到 SSE 后本地改：会让 channel / server / Web 产生多份语义

因此，本 RFC 建议把触发点放在 server 的 turn 完成收口路径中：

1. 收到完成态 turn
2. 更新 `last_active_at`
3. 增加 session 的“自上次重命名以来用户对话计数”
4. 判断当前 session 是否允许参与自动重命名
5. 判断当前 session 是否达到自动重命名阈值
6. 若达到阈值，则异步调度 AI 重命名任务
7. 重命名成功后更新 store
8. 发出显式 `session_updated` 事件

如果这一步失败：

- 不影响 turn 完成主流程
- 不回滚 session 或 turn
- 只记录错误并跳过本次自动重命名

### 6. 自动重命名采用 `3~5` 次用户对话的抖动节奏

本 RFC 不采用“只重命名一次”的首版策略，而是采用更接近真实聊天产品的渐进式重命名：

- `default -> auto`：允许
- `auto -> auto`：允许，但仅在节奏窗口命中时触发
- `manual`：永不自动覆盖
- `channel`：默认永不自动覆盖

具体节奏要求：

- 不在每个 turn 完成后都触发重命名
- 使用 `3~5` 次用户对话一个窗口的抖动策略
- 每个 session 独立维护下次触发阈值
- 被标记为 `auto_rename_policy = disabled` 的 session 永远不进入这套节奏系统

建议实现：

- session 元信息新增 `rename_after_user_turns`
- 初始值在 `[3, 5]` 范围内随机选择一个整数
- 每当用户 turn 成功进入 session，就递增 `user_turn_count_since_last_rename`
- 当计数达到阈值时，调度一次 AI 重命名
- 重命名任务结束后：
  - 将计数归零
  - 再次生成下一个 `[3, 5]` 的阈值

这样做的好处是：

- 语义稳定
- 标题会随着会话主题逐渐收敛，而不是永远停留在第一轮
- 不会因为每轮都触发而造成标题闪烁
- 不同 session 的重命名时机自然错开，避免批量风暴

为了保持工程实现简单，本 RFC 不要求真正的概率模型；只要求最终触发频率落在“每 3~5 次用户对话触发一次”的行为区间内。

### 7. 客户端同步统一使用 `session_updated` 事件

当前 Web 对 session 元信息主要依赖：

- 首次 `GET /api/sessions`
- `session_created`
- `session_deleted`

若自动重命名只通过“等下一次整表刷新”生效，会造成明显迟滞，也会让不同客户端刷新节奏不一致。

因此，本 RFC 不建议新增只承载重命名的特化事件，而是建议新增统一 session 元信息增量事件：

- `session_updated`

建议 payload：

```json
{
  "session_id": "20260328_abcd1234",
  "title": "设计 session 自动重命名 RFC",
  "title_source": "auto",
  "updated_at": "2026-03-28T13:20:00Z",
  "last_active_at": "2026-03-28T13:18:42Z",
  "model": "model-primary"
}
```

客户端收到后应：

- 仅更新对应 session 列表项
- 若该 session 当前处于激活态，同步更新相关本地 snapshot 中可见标题投影
- 若标题发生变化，启动一次局部打字机动画
- 若只有 `last_active_at` 变化，则只刷新时间文案，不触发标题动画
- 不必为此整表重拉 `GET /api/sessions`

这条事件不替代列表接口，而是作为增量投影信号。

之所以采用 `session_updated` 而不是 `session_renamed`：

- 后续 session 元信息变化都可以复用同一事件面
- 避免随着字段增加不断扩新的 SSE 事件种类
- 更贴合现有 session list item 的投影更新模型

### 7.1 `session_updated` 的发射时机

建议至少在以下场景发射：

- session 自动重命名成功
- session 标题来源变化
- `last_active_at` 更新且值得对外投影
- 未来显式手动重命名成功

为了避免 SSE 噪音，本 RFC 建议：

- 同一次 session 元信息写入只发一条 `session_updated`
- 若一次 turn 既更新 `last_active_at` 又触发自动重命名，优先合并成单个事件

### 8. 共享 helper 应该收口在后端可复用层，而不是前端组件里

自动重命名的核心逻辑应收口为后端共享 helper，例如：

- 标题来源枚举
- `should_schedule_session_rename(...)`
- `next_session_rename_threshold(...)`
- `should_emit_session_updated(...)`

它们应该落在能够被 session manager 直接复用、并且容易做 Rust 单测的边界里，而不是写进：

- Web sidebar 组件
- SSE handler 的局部逻辑
- channel adapter 自己的一份副本

这样可以保证：

- 所有 session 入口行为一致
- 测试只需要锁定一份规则
- 未来若替换成 LLM 标题生成，也只需替换一个策略边界

## Data Model

### `sessions` 表新增字段

建议新增：

- `title_source TEXT NOT NULL DEFAULT 'default'`
- `auto_rename_policy TEXT NOT NULL DEFAULT 'enabled'`
- `last_active_at TEXT NOT NULL`
- `user_turn_count_since_last_rename INTEGER NOT NULL DEFAULT 0`
- `rename_after_user_turns INTEGER NOT NULL DEFAULT 3`

迁移要求：

- 老数据若只存在 `title`，则需要根据值回填
- 对历史数据的保守迁移建议是：
  - `title == ''` 或 `title == 'New session'` -> `default`
  - 其余值 -> `manual`
- `auto_rename_policy` 缺失时：
  - 普通本地 session 保守回填为 `enabled`
  - 若未来能识别其来源为 channel，可回填为 `inherit` 或 `disabled`
- `last_active_at` 缺失时，默认回填为 `updated_at`
- `user_turn_count_since_last_rename` 缺失时回填为 `0`
- `rename_after_user_turns` 缺失时回填为 `3`

这是一个保守但安全的迁移策略：宁可少自动改，也不要误覆盖旧用户标题。

### `SessionRecord`

`SessionRecord` 建议扩展为：

- `id`
- `title`
- `title_source`
- `auto_rename_policy`
- `created_at`
- `updated_at`
- `last_active_at`
- `model`

并补充明确构造入口，避免后续在 app 壳层手写字符串。

对于 `user_turn_count_since_last_rename` 与 `rename_after_user_turns`，若不希望直接暴露到前端列表类型，也可以作为 store 内部字段存在，不进入公开 API。

## Runtime and Server Semantics

### 自动重命名判定条件

一个 turn 完成后，仅当以下条件同时满足时才尝试自动重命名：

1. session 存在且可写
2. 当前 session `auto_rename_policy` 允许参与自动重命名
3. 当前 session `title_source` 为 `default` 或允许被自动更新的来源
4. turn outcome 为稳定完成态；取消或失败默认不触发
5. 当前 session 的用户对话计数已经命中本轮阈值
6. 当前 session 没有正在进行中的重命名任务
7. 最近对话片段足以生成有效标题
8. 生成后的标题与现有标题不同

若任一条件不满足，则直接跳过。

### 重命名任务并发约束

每个 session 任意时刻最多只允许一个 pending rename job。

若某个 session 在 rename job 运行期间又完成了新 turn：

- 仍正常更新 `last_active_at`
- 仍正常累加用户对话计数
- 但不再额外并发启动第二个 rename job

待当前 rename job 完成后，再由下一次 turn 完成路径继续判定是否需要新一轮重命名。

### Channel / Profile 策略来源

对于 channel 创建的 session，是否允许自动重命名不应在 turn 现场临时猜测，而应在 session 创建时就决议并固化到 session 元信息里。

建议职责分工：

- `channel-bridge` / adapter catalog：声明 transport 级默认策略
- channel profile：可覆盖 transport 默认策略
- `apps/agent-server`：在创建 session 时把最终策略写入 session record
- session manager：后续只读取 session 上已经固化的 `auto_rename_policy`

这样可以避免每次 turn 完成时再反查 channel profile、transport 或入口上下文。

### turn outcome 与触发关系

推荐首版只在以下 outcome 触发：

- `succeeded`

默认不在以下 outcome 触发：

- `failed`
- `cancelled`
- `waiting_for_question`

原因：

- `failed` 可能对应半截任务，不适合作为主题代表
- `cancelled` 说明主题未稳定
- `waiting_for_question` 本轮尚未闭合，可能只是澄清阶段

### 最近对话窗口建议

为了让 AI 标题既能反映近期主题，又不至于成本过高，本 RFC 建议重命名输入窗口优先选取：

- 最近 `3~5` 个用户 turn
- 最多不超过固定字符预算

推荐优先级：

1. 最近用户消息
2. 必要的 assistant 简短总结或截断摘要
3. 明确忽略大段 tool 输出

## Frontend Semantics

### Session 列表新增最近活跃时间

前端 session 列表项建议同时展示：

- 主标题：session title
- 次级元信息：最近活跃时间

推荐呈现形态：

- 列表主行显示标题
- 最近活跃时间固定显示在行尾，始终可见
- 当标题过长时，优先截断标题本身，不挤掉尾部时间
- 时间使用弱化样式，例如 `just now`、`5 minutes ago`、`2 hours ago`

该时间展示应来源于 `last_active_at`，而不是 `updated_at`。

建议排序语义保持不变：

- session 列表是否按 `created_at`、`updated_at` 或其他规则排序，不由本 RFC 强制改变
- `last_active_at` 当前只作为展示元信息，不自动改变列表排序语义

这样可以把“列表排序策略”与“最近活跃展示”解耦，避免本轮把 session list 的交互语义一起带大。

### 标题变化的打字机效果

当前端收到 `session_updated` 且发现某条 session 的 `title` 发生变化时：

- 只对该条 session 执行一次局部标题动画
- 动画仅作用于文本展示，不改变权威数据
- 动画结束后展示完整标题

建议约束：

- 只在标题真实变化时触发
- 只对当前可见 session 列表项触发
- 单次动画时长要短，避免影响 sidebar 可读性
- 若短时间内同一 session 又收到新标题，新的动画应覆盖旧动画，而不是排队播放

推荐实现方式：

- Web store 维护一个按 `session_id` 索引的临时 animation state
- 该 state 只存：
  - `target_title`
  - `rendered_title`
  - `animating`
- 组件优先渲染 `rendered_title`；动画结束后回到权威 `title`

该动画不应写回服务端，也不应进入 session 权威数据模型。

### 打字机动画状态机建议

前端可将单个 session 项的标题动画建模为轻量状态机：

- `idle`
- `animating`
- `settled`

建议流转：

1. 收到 `session_updated`
2. 若 `title` 未变化，则保持 `idle`
3. 若 `title` 变化，则进入 `animating`
4. 逐帧推进 `rendered_title`
5. 动画完成后进入 `settled`
6. 下次标题再变时，重新进入 `animating`

约束：

- `settled` 可以在实现上直接等价回收为 `idle`
- 若组件卸载或 session 被删除，动画状态直接丢弃
- 若用户系统开启 reduced motion，`animating` 可直接跳到最终态

## API and Event Surface

### 现有列表接口

`GET /api/sessions` 返回结构需要同步包含：

- `title_source`
- `last_active_at`

这样客户端才能判断某个标题是否为自动生成，未来也更方便做 UI 区分。

### 新增 SSE 事件

新增：

- `session_updated`

payload 建议：

```json
{
  "session_id": "20260328_abcd1234",
  "title": "修复 trace dashboard 热路径",
  "title_source": "auto",
  "auto_rename_policy": "enabled",
  "updated_at": "2026-03-28T13:20:00Z",
  "last_active_at": "2026-03-28T13:18:42Z",
  "model": "model-primary"
}
```

该事件应只在 session 元信息实际变化时发送，不为“判定后结果没变”的情况发空事件。

### 可能的内部重命名接口

本 RFC 不要求暴露额外公网 API，但 server 内部建议有一个明确调用面来执行 AI 标题生成，例如：

- `SessionManagerHandle::schedule_auto_rename(...)`
- `SessionAutoRenameService::run_once(...)`

这样 turn 完成路径只负责“判定与调度”，实际模型调用和规范化逻辑由独立服务承接。

### Channel / Profile 配置建议

本 RFC 建议在 channel transport 定义或 profile 配置里预留自动重命名开关，例如：

```json
{
  "auto_rename_sessions": false
}
```

或更显式地：

```json
{
  "session_auto_rename_policy": "disabled"
}
```

是否采用布尔值还是枚举值，可以在实现时按现有 channel schema 风格决定；但语义上必须能表达：

- 显式开启
- 显式关闭
- 跟随 transport 默认值

## Reference Shapes

本节不是最终代码签名，但用于把实现边界收紧到足够接近代码的程度，减少后续落地时的歧义。

### 决策快照

为了避免后续实现阶段重新发散，本 RFC 当前建议先按以下固定决策落第一版：

- session 自动重命名默认只对 `Web` 与 `self-chat` 启用
- channel 创建的 session 默认 `auto_rename_policy = disabled`
- 自动重命名触发节奏为每 `3~5` 次用户对话一次
- AI 重命名作为后台 side effect，不阻塞 turn 完成
- 重命名请求单独打 trace kind：`session_rename`
- 前端统一消费 `session_updated`
- 前端标题动画只在标题真实变化时触发
- `last_active_at` 只表示对话活跃，不因单纯标题变化而更新

### Rust: session 标题来源与自动重命名策略

建议形状：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTitleSource {
    Default,
    Auto,
    Manual,
    Channel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionAutoRenamePolicy {
    Enabled,
    Disabled,
    Inherit,
}
```

若实现时希望避免 `Inherit` 进入最终 session record，也可以只在 channel/profile 配置层保留 `Inherit`，而写入 `sessions` 表时只落最终决议后的：

- `enabled`
- `disabled`

这是一个实现可选项，但 RFC 更推荐“session record 里存最终决议值”，这样后续 turn 路径无需再反查来源配置。

### Rust: `SessionRecord` 建议扩展

建议形状：

```rust
pub struct SessionRecord {
    pub id: String,
    pub title: String,
    pub title_source: SessionTitleSource,
    pub auto_rename_policy: SessionAutoRenamePolicy,
    pub created_at: String,
    pub updated_at: String,
    pub last_active_at: String,
    pub model: String,
}
```

若 store 内部还需要调度字段，建议不要混进公开 `SessionRecord`，而是下沉为内部 row model 或独立字段访问接口，例如：

```rust
struct SessionRenameState {
    pub user_turn_count_since_last_rename: u32,
    pub rename_after_user_turns: u32,
    pub rename_job_running: bool,
}
```

其中 `rename_job_running` 不一定要落库；若实现上只需要进程内并发保护，也可保留在 session manager 内存态。

### Rust: `session_updated` 事件建议形状

建议 server 侧 SSE payload 形状：

```rust
pub enum SsePayload {
    SessionCreated {
        session_id: String,
        title: String,
    },
    SessionUpdated {
        session_id: String,
        title: String,
        title_source: SessionTitleSource,
        auto_rename_policy: SessionAutoRenamePolicy,
        updated_at: String,
        last_active_at: String,
        model: String,
    },
    SessionDeleted {
        session_id: String,
    },
    // ...
}
```

如果后续觉得 `model` 太吵，也可以从 `SessionUpdated` 中移除，但 `title`、`title_source`、`updated_at`、`last_active_at` 应视为核心字段。

### Rust: 自动重命名调度 helper 建议

建议至少收口这些 helper：

```rust
fn should_count_turn_for_auto_rename(turn: &TurnLifecycle) -> bool;

fn should_schedule_session_rename(
    record: &SessionRecord,
    rename_state: &SessionRenameState,
    turn: &TurnLifecycle,
) -> bool;

fn next_session_rename_threshold() -> u32;

```

建议职责：

- `should_count_turn_for_auto_rename(...)`：只判断某个 turn 是否计入“用户对话次数”
- `should_schedule_session_rename(...)`：综合 policy、title_source、turn outcome、节奏阈值、并发状态做最终判定
- `next_session_rename_threshold()`：返回 `3..=5` 的下次触发阈值

### Rust: 自动重命名 prompt 契约建议

为了避免提示词在实现期变成随手拼接字符串，session 自动重命名 prompt 现已收口到共享 `agent-prompts` crate，例如：

```rust
pub struct TitleGeneratorPromptContext {
    pub current_title: String,
    pub title_source: String,
    pub recent_user_turns: Vec<String>,
}

pub fn render_title_generator_prompt(context: TitleGeneratorPromptContext) -> String;
```

建议 prompt 约束至少包含：

- 你是 title generator，只输出 thread title
- 必须使用与用户消息相同的语言
- 标题需要简短、自然、可检索
- 不要输出解释、引号、编号或 markdown
- 直接输出标题本身

若实现侧后续改成结构化输出，也建议保持逻辑等价，而不是把“标题生成规范”重新写散。

### Rust: 重命名任务服务建议

建议把实际 AI 重命名任务收口在独立服务对象，而不是塞回 turn worker：

```rust
pub struct SessionAutoRenameService {
    // store / model builder / trace recorder / event sender ...
}

impl SessionAutoRenameService {
    pub async fn run_once(&self, session_id: &str) -> Result<Option<SessionRenameResult>, Error>;
}

pub struct SessionRenameResult {
    pub title: String,
    pub title_source: SessionTitleSource,
    pub updated_at: String,
}
```

其中 `Ok(None)` 表示：

- 条件重新检查后发现不需要改名
- 或 AI 输出无效，被规范化逻辑丢弃

### TypeScript: session list item 建议形状

前端 list item 建议扩展为：

```ts
export type SessionTitleSource =
  | "default"
  | "auto"
  | "manual"
  | "channel"

export type SessionAutoRenamePolicy =
  | "enabled"
  | "disabled"
  | "inherit"

export type SessionListItem = {
  id: string
  title: string
  title_source: SessionTitleSource
  auto_rename_policy: SessionAutoRenamePolicy
  created_at: string
  updated_at: string
  last_active_at: string
  model: string
}
```

如果服务端最终不把 `inherit` 暴露给 session record，则前端类型也应同步收窄，而不是保留虚假状态。

### TypeScript: `session_updated` 事件建议形状

建议新增：

```ts
type SseEvent =
  | { type: "session_created"; data: { session_id: string; title: string } }
  | {
      type: "session_updated"
      data: {
        session_id: string
        title: string
        title_source: SessionTitleSource
        auto_rename_policy: SessionAutoRenamePolicy
        updated_at: string
        last_active_at: string
        model: string
      }
    }
  | { type: "session_deleted"; data: { session_id: string } }
  // ...
```

### TypeScript: 前端标题动画状态建议形状

建议前端 store 持有一个临时动画映射，而不是修改权威 `sessions` 数据的 `title` 语义：

```ts
export type SessionTitleAnimationState = {
  targetTitle: string
  renderedTitle: string
  animating: boolean
  startedAtMs: number
}

type ChatStore = {
  sessions: SessionListItem[]
  sessionTitleAnimations: Record<string, SessionTitleAnimationState>
}
```

建议渲染优先级：

1. 若 `sessionTitleAnimations[id]?.animating === true`，渲染 `renderedTitle`
2. 否则渲染 `session.title`

也可以把动画状态单独下沉为 helper，例如：

```ts
function startSessionTitleTypingAnimation(
  sessionId: string,
  nextTitle: string
): void
```

这样 `handleSseEvent("session_updated")` 只负责判定是否触发动画，不需要自己管理逐帧逻辑。

### 最近活跃时间格式化建议

本 RFC 建议前端增加一个稳定 helper，而不是在组件里手写散落逻辑：

```ts
function formatSessionLastActiveAt(iso: string, now: Date): string
```

推荐目标输出：

- `刚刚`
- `5 分钟前`
- `2 小时前`
- `昨天`
- `3 天前`

后续如果需要国际化，再把它进一步下沉到统一 i18n/formatting 入口。

### 后端调度顺序建议

一个典型 turn 完成后的推荐顺序如下：

1. 持久化 turn 完成事实
2. 更新 session `last_active_at`
3. 增加 session 用户对话计数
4. 若命中阈值且 policy 允许，则尝试占用 rename job slot
5. 立即发一条合并后的 `session_updated`（若 `last_active_at` 已变化）
6. 后台执行 AI 重命名
7. 若标题更新成功，再发第二条 `session_updated`

这个顺序意味着：

- 最近活跃时间可以比标题更早到达前端
- 标题更新保持异步，不阻塞当前轮
- 前端可能在同一 session 上依次收到“活跃时间变了”和“标题变了”两条事件

若后续希望进一步减少事件量，也可以把第 5 步改为仅在标题未触发时才发，但首版不强制。

### 后端伪流程建议

一个更接近代码的伪流程如下：

```rust
async fn on_turn_completed(session_id: &str, turn: &TurnLifecycle) {
    persist_turn(turn).await?;

    if should_count_turn_for_auto_rename(turn) {
        touch_session_last_active_at(session_id).await?;
        increment_user_turn_count_since_last_rename(session_id).await?;
        emit_session_updated_for_activity(session_id).await?;
    }

    let record = load_session_record(session_id).await?;
    let rename_state = load_session_rename_state(session_id).await?;

    if !should_schedule_session_rename(&record, &rename_state, turn) {
        return;
    }

    if !try_mark_rename_job_running(session_id).await? {
        return;
    }

    spawn(async move {
        let result = auto_rename_service.run_once(session_id).await;
        clear_rename_job_running(session_id).await.ok();

        match result {
            Ok(Some(rename)) => {
                reset_user_turn_count_since_last_rename(session_id).await.ok();
                set_next_rename_threshold(session_id, next_session_rename_threshold()).await.ok();
                emit_session_updated_for_title_change(session_id, rename).await.ok();
            }
            Ok(None) => {}
            Err(error) => {
                record_session_rename_failure_trace(session_id, error).await.ok();
            }
        }
    });
}
```

这个伪流程里最关键的是两点：

- `last_active_at` 的更新和 AI 重命名彻底拆开
- rename job 的并发占位必须在真正 spawn 前完成

## Compatibility and Migration

### 向后兼容要求

第一版实现必须保证：

- 老前端即使不识别 `session_updated`，也不会导致主聊天链路出错
- 老 session 数据在迁移后仍可正常列出、切换、删除
- 未启用自动重命名时，session 行为应尽量与当前版本一致

### 前后端灰度兼容

因为 `session_updated` 是新增事件，建议前端处理时遵循“忽略未知字段、兼容旧事件”的原则：

- 旧服务端：没有 `session_updated`，前端仍可工作
- 新服务端 + 旧前端：旧前端忽略未知事件，不应报错
- 新服务端 + 新前端：完整体验生效

### 历史 session 的策略回填

建议历史数据回填原则保持保守：

- 对无法判断来源的旧 session，默认视为本地 session，`auto_rename_policy = enabled`
- 若某批历史 session 明确来自 channel 且已有上游标题，可在迁移脚本中回填为 `disabled`

是否做更细粒度回填，可以留待实现时基于现有 store / binding 信息再决定，但默认行为必须可预测。

## Event Coalescing Guidance

`session_updated` 会同时承载活跃时间更新和标题变化，因此需要明确事件合并策略，避免实现时两边乱发。

推荐规则：

- `last_active_at` 变化：允许立即发送 `session_updated`
- 标题变化：必须发送 `session_updated`
- 同一事务里若两者一起变化：只发一条
- turn 刚完成先发了活跃事件，随后后台重命名成功：可以再发一条标题事件

也就是说，本 RFC 允许一轮对话带来两条 `session_updated`：

- 第一条：活跃时间变化
- 第二条：标题变化

这是可接受的，因为两者的发生时机本来就不同。

## UI Notes

### 最近活跃时间刷新频率

前端显示相对时间时，不需要为了 sidebar 文案精确到秒而高频重渲染。

建议：

- 列表打开期间，用低频定时器刷新相对时间文案
- 刷新粒度控制在“分钟级”即可

这样可以避免“刚刚 / 1 分钟前”这类文本驱动整个 sidebar 高频更新。

### 打字机动画的稳定性约束

为了避免动画把 sidebar 搞得很飘，建议额外加两条约束：

- 动画过程中不改变 session item 的布局层级和高度
- 动画文字应直接在已有标题容器内进行，不额外插入占位骨架或过渡卡片

也就是说，这个效果应该是“标题字符逐步出现”，不是“整个 session row 重新 mount”。

## Testing Guidance

本 RFC 建议至少覆盖以下测试面：

### Rust / store

- `sessions` 表迁移后旧数据能正确回填 `title_source`
- `last_active_at` 缺省值正确回填
- `auto_rename_policy` 缺省值正确回填

### Rust / session manager

- 普通 Web session 在计数命中后会调度重命名
- `auto_rename_policy = disabled` 的 session 永不调度重命名
- `manual` / `channel` 标题来源不会被自动覆盖
- 同一 session 不会并发跑两个 rename job
- rename job 失败不影响 turn 完成主链
- `session_updated` 在 `last_active_at` 更新时正确发出
- `session_updated` 在标题更新成功时正确发出

### Rust / rename normalization

- 空标题被拒绝
- 带解释前缀的标题被清洗
- 超长标题被截断
- 纯标点或无意义标题被拒绝

### Web / store

- 收到 `session_updated` 后正确增量更新 session 列表项
- 仅 `last_active_at` 变化时不触发标题动画
- `title` 变化时启动并完成一次打字机动画
- 同一 session 短时间内二次标题更新会覆盖旧动画

### Web / rendering

- sidebar 能展示最近活跃时间
- reduced motion 下标题变化会直接切到最终态
- session 删除后残留动画状态会被清理

## Alternatives Considered

### 1. 由 Web 端本地从首条消息直接生成标题

未采纳。

原因：

- 只有 Web 生效，外部 channel 和其他客户端没有这条语义
- 标题不再由 server/store 主导，跨重启一致性差
- 容易和未来手动重命名、channel 初始标题冲突

### 2. session 创建时立刻用首条 prompt 命名

未采纳。

原因：

- session 创建时往往还没有用户输入
- 会把创建路径和标题生成耦合在一起
- 若未来标题生成失败，可能干扰 session 创建成功率

### 3. 每个 turn 完成后都立即触发 AI 重命名

未采纳。

原因：

- 过于昂贵
- 标题会频繁变化
- 前端动画会不断打断阅读
- trace 与 provider 成本会被放大

因此本 RFC 采用 `3~5` 次用户对话一轮的抖动节奏。

### 4. 不增加 `title_source`，只靠当前标题是否等于 `New session` 判断

未采纳。

原因：

- 无法表达 `auto` 与 `manual` 的区别
- 用户若把标题手改成 `New session` 会被误判
- 历史兼容和未来扩展都很脆弱

### 5. 所有 channel 一律和 Web 一样参与自动重命名

未采纳。

原因：

- 一些 channel 已经有稳定的上游命名来源
- 一些 channel 成本和延迟约束更强
- 一些 channel 根本不希望本地 AI 改写会话主题
- 不同 transport 的产品语义差异很大，不适合强制统一

因此本 RFC 采用“channel/profile 可声明是否参与自动重命名”的策略层。

## Risks and Mitigations

### 风险 1：误覆盖用户标题

缓解：

- 引入 `title_source`
- 自动重命名仅允许覆盖 `default` / 允许自动覆盖的来源
- `manual` / `channel` 一律锁定

### 风险 2：标题质量差，看起来像截断 prompt

缓解：

- 使用 AI 生成 + 共享 title prompt 约束
- 保留最基本的空值跳过判定
- 控制上下文窗口与输出长度

### 风险 2.1：AI 重命名额外增加成本

缓解：

- 只按 `3~5` 次用户对话触发一次
- 将请求单独标记为 `session_rename`
- 后续可加全局开关、限流和预算控制

### 风险 3：自动重命名失败影响 turn 主链

缓解：

- 标题生成与落库失败不得影响 turn 完成
- 将其视为附加型 side effect
- 仅记录错误并跳过

### 风险 4：客户端标题更新不同步

缓解：

- 新增 `session_updated` SSE
- `GET /api/sessions` 仍作为全量权威恢复接口

### 风险 4.1：打字机动画影响可读性或造成闪烁

缓解：

- 动画只在标题真实变化时触发
- 限制动画持续时间
- 新标题到达时覆盖旧动画，不排队
- 允许前端在 reduced-motion 场景下降级为无动画切换

### 风险 5：历史数据迁移导致过度自动改名

缓解：

- 历史回填默认保守地把非默认标题标成 `manual`
- 不对历史 session 做主动批量重命名

## Open Questions

- `channel` 来源在显式启用自动重命名后，是否仍默认保留 `title_source=channel` 的不可覆盖语义，还是允许切到 `auto`
- `waiting_for_question` 前是否要允许把当前未闭合主题纳入重命名窗口
- AI 重命名是否始终复用当前 session model，还是后续抽出更便宜的专用模型
- `session_updated` 是否还应承载未来的手动 pin/star/sort 元信息
- 最近活跃时间的前端相对时间文案是否需要统一国际化入口

## Rollout Plan

### Phase 1: Store 与协议补齐

- 为 `sessions` 表增加 `title_source`
- 为 `sessions` 表增加 `auto_rename_policy`
- 为 `sessions` 表增加 `last_active_at`
- 为内部重命名调度增加计数字段与阈值字段
- 扩展 `SessionRecord` 与对应 list/get/update API
- 完成历史数据保守迁移

### Phase 2: Server AI 自动重命名逻辑

- 在 turn 完成收口点更新 `last_active_at` 与对话计数
- 接入 channel/profile → session 的自动重命名资格决议
- 实现 `3~5` 次用户对话的重命名调度
- 实现 AI 标题生成与结果规范化 helper
- 为重命名请求补 trace kind `session_rename`

### Phase 3: 客户端增量同步

- 新增 `session_updated` SSE 事件
- Web store 增量更新 session 列表项
- Web session 列表展示 `last_active_at`
- Web 为标题更新加入打字机动画
- 保持 `GET /api/sessions` 作为冷启动和重同步入口

### Phase 4: 手动重命名与后续策略评估

- 视产品需求补手动重命名入口
- 评估不同 channel 的默认自动重命名策略
- 评估是否需要专用低成本标题模型与预算开关

## Success Criteria

- 活跃 session 会在持续对话过程中逐步收敛到更贴切的标题，而不是长期停留在 `New session`
- 自动重命名平均频率符合“每 `3~5` 次用户对话触发一次”的设计目标
- 用户手动标题不会被自动重命名覆盖
- 被禁用自动重命名的 channel session 不会触发 AI 重命名请求
- 外部 channel 创建的带来源标题 session 不会被误改
- 标题更新不会增加 turn 主链失败率
- Web 可通过 `session_updated` 在不整表刷新的情况下看到标题和最近活跃时间变化
- 标题变化的打字机效果不会造成明显 sidebar 抖动或持续闪烁
- 自动重命名、`session_updated` 投影、最近活跃时间与前端动画都具备对应测试覆盖
