# 架构说明

> 本文件只保留 **稳定结构、模块职责与 ownership 边界**。当前进度看 `docs/status.md`，具体历史看 `docs/evolution-log.md`。

- Last verified: `2026-03-30`

## 总体目标

`aia` 的架构核心是三件事：

1. **统一运行时**：Web、CLI、桌面壳、外部客户端尽量复用同一套 Rust 运行时主链
2. **append-only session tape**：会话事实保持可追溯、可恢复、可重建
3. **单一内部工具协议**：内部只维护一套工具定义，对外再做协议映射

所以这个项目不是“Web 项目 + 一个后端”，而是：

- 共享 Rust 核心层
- 一个规范化控制面 `agent-server`
- 一个当前主承接客户端 `apps/web`

## 分层视图

```text
apps/web
   │  HTTP + SSE
   ▼
apps/agent-server
   │  bridge / control-plane
   ▼
crates/agent-runtime
   │  turn loop / tools / events / compression
   ├───────────────┬───────────────┬───────────────┐
   ▼               ▼               ▼               ▼
session-tape   builtin-tools   openai-adapter   agent-store
   │               │               │               │
append-only      tool exec      provider I/O      SQLite metadata / trace
session facts                                       
```

## 模块职责

### 1. `aia-config`

负责应用级共享默认值与稳定约定：

- `.aia/` 目录与默认路径
- server 默认 bind 地址、请求超时、事件缓冲等应用级默认值
- 默认 session 标题、trace / span / prompt-cache 稳定前缀
- 统一 user agent 组装 helper

**不负责**：

- provider 业务
- 运行时编排
- 协议映射
- 算法阈值本身

### 2. `agent-core`

负责纯领域抽象：

- 消息、角色、上下文窗口
- 模型身份与能力
- 工具定义、工具调用、共享 schema 能力
- completion request / usage 等共享类型
- question / session interaction 等共享协议类型

**原则**：不泄漏具体 provider、前端、channel 或 app 壳细节。

### 3. `agent-prompts`

负责共享 prompt 资产与装配逻辑：

- system prompt 组合入口
- aia agents 模板
- title generator prompt
- 工具 description 文本

它是共享 prompt 层，不是 app 壳的字符串垃圾桶。

### 4. `session-tape`

负责 append-only 会话事实：

- 扁平条目 `{id, kind, payload, meta, date}`
- anchor / handoff
- fork / merge
- 查询切片与上下文重建
- pending question 等 append-only 交互事实的事件记录
- jsonl 落盘与重放

**关键边界**：

- tape 记录事实
- 不替代 SQLite 元信息
- 不承接前端投影逻辑

### 5. `agent-runtime`

负责真正的 agent loop：

- 追加用户输入
- 组装上下文
- 调用模型
- 执行工具
- 落盘工具与轮次事实
- 发布 runtime event
- 承接 stop/cancel / compression 主链

它是共享运行时，不应重新被 app 壳分叉成第二套“简化版运行时”。

### 6. `builtin-tools`

负责内建工具定义与执行：

- Shell / Read / Write / Edit / ApplyPatch / Glob / Grep
- CodeSearch / WebSearch
- TapeInfo / TapeHandoff
- Question

**边界**：

- 放工具定义和执行逻辑
- 不把 app 壳控制面、前端 UI 或 provider 协议塞进工具实现

### 7. `openai-adapter`

负责 provider 边缘适配：

- OpenAI Responses
- OpenAI 兼容 Chat Completions
- async `reqwest` HTTP / SSE streaming
- prompt cache 映射
- usage / cached tokens 回填

**原则**：OpenAI 特有概念停留在边缘层，不扩散回核心层。

### 8. `provider-registry`

负责 provider 聚合模型与行为语义：

- provider profile
- 一 provider 多 model
- 当前模型选择语义

它是 provider 领域模型层，不应重新承担 app 壳持久化职责。

### 9. `agent-store`

负责本地 SQLite 持久化：

- session 元信息
- provider / provider_models
- channel profile 与动态索引
- trace / overview / dashboard 聚合

**关键边界**：

- SQLite 存结构化元信息与聚合数据
- 不替代 session tape
- 不承接路由与 UI 投影

### 10. `channel-bridge`

负责 channel 共享抽象：

- channel profile façade
- adapter catalog
- runtime host / supervisor / event 边界
- external conversation → `session_id` 绑定恢复
- 回执幂等、turn 前预压缩等 bridge helper

它应该停留在“transport-neutral bridge”层级，不承接具体平台协议。

### 11. `channel-feishu` / `channel-weixin` / `weixin-client`

这些 crate 负责边缘 transport：

- `channel-feishu`：Feishu 长连接、回复控制、平台适配
- `channel-weixin`：Weixin transport、轮询 worker、消息映射
- `weixin-client`：微信私有协议 client 与媒体 helper

**原则**：平台协议细节停留在边缘 crate，不回流到 `agent-server` 或共享核心层。

## 应用壳职责

### `apps/agent-server`

`agent-server` 是 **控制面与桥接层**，职责是：

- 暴露 HTTP + SSE API
- 管理 session manager 与 runtime 所有权
- 暴露可嵌入的 bootstrap / run façade
- 作为 Web、CLI self、channel 的统一入口
- 承接 runtime host 能力，例如 pending question 控制面

它不应该变成：

- 第二套运行时核心
- 充满业务规则的巨石入口
- 重新定义工具协议、provider 协议或会话语义的地方

### `apps/web`

`apps/web` 是当前主工作台，职责是：

- 展示 session / history / current turn / trace / channels / settings
- 消费 SSE 事件流
- 承接输入、配置和恢复逻辑
- 负责 UI 状态与表现层动画

它不应该：

- 重写 agent loop
- 自己发明另一套会话语义
- 绕过 server 直接与共享核心通信

## 关键 ownership 约束

### 1. Session 事实 vs 元信息

- **事实**：session tape（append-only jsonl）
- **元信息**：SQLite
- **投影**：server snapshot / SSE / Web store

派生投影必须可重建，不能反过来覆盖事实源。

### 2. 工具协议

- 内部只有一套工具定义
- 外部模型家族差异只能在映射层解决
- 工具名要稳定，不把底层执行器名字暴露成公共协议

### 3. Runtime 与 app 壳

- runtime 承担执行语义
- app 壳承担桥接与控制面
- 若某段逻辑可以在多个 app / client 复用，应优先下沉到共享层

### 5. Widget Host 协议

- 当前 `WidgetRenderer` 已经存在，但它不应长期停留在“Web 特判 HTML sandbox”层级
- widget 的宿主语义、bridge 动作、生命周期阶段与 capability 边界，应优先下沉到共享协议，而不是继续堆在 `apps/web`
- `apps/web` 负责 widget host 的渲染与交互承接；`apps/agent-server` 负责 current-turn / SSE / replay 投影；共享 crate 负责稳定协议与 capability 语义
- dashboard / pin / export / cross-widget orchestration 属于后续产品层能力，不应在第一波 host/runtime 收口时混入基础协议层

### 4. Channel 边界

- transport 专属协议留在 channel crate
- 通用 channel host / event / binding 语义留在 `channel-bridge`
- `agent-server` 只做宿主装配，不保留平台细节实现

## 当前架构热点

当前最值得继续收口的实现热点是：

1. `apps/agent-server/src/session_manager/turn_execution.rs`
   - runtime ownership / return-path 复杂
2. server 驱动面与共享 runtime 之间还能继续下沉的辅助逻辑
3. 工具协议对外映射与 MCP 接入边界
4. widget host 协议仍处于从 `WidgetRenderer` Web 特判向共享语义收口的早期阶段；当前已开始 capability 解耦和最小协议建模，但还未完成 runtime/server/Web 的统一投影

这些是当前的结构热点，不应再被新的客户端表层需求挤掉优先级。

## 与其他文档的关系

- `docs/requirements.md`：回答“想做什么、当前不做什么”
- `docs/status.md`：回答“现在做到哪了”
- `docs/todo.md`：回答“还没做什么”
- `docs/rfc/*`：回答“为什么这样设计”
