# aia：第一阶段架构

## 目标

这份骨架先解决三件以后最难改的事：

1. **统一代理核心**：Web 界面和桌面壳共享同一套运行时，不重复造轮子。
2. **可追加的会话磁带**：把上下文、压缩、交接、分叉都建立在扁平的 `{id, kind, payload, meta, date}` 事件流之上，对齐 republic 数据模型。
3. **可兼容的工具协议**：内部只维护一套工具定义，向外可映射到不同模型或协议。

## 为什么先做库，不先做界面

README 里真正难的是这些能力：

- 模型人格差异感知
- 工具系统、子代理、异步子代理
- 增量压缩与交接
- 兼容不同外部工具规范

这些如果先揉在界面里，后面会很难拆。第一阶段因此采用“库优先”：

- `aia-config`：应用级共享默认值与路径约定（如 `.aia/` 下的 providers / session / store 路径、server 默认标识），避免这些固定约定散落在 app 壳与存储 crate 中
- `agent-core`：领域模型与协议边界
- `session-tape`：扁平条目磁带（`{id, kind, payload, meta, date}`）、轻量锚点、handoff 事件、查询切片、fork / merge 与重建状态
- `agent-runtime`：运行时编排与最小 turn 执行
- `provider-registry`：provider 资料、活动项与本地持久化
- `openai-adapter`：首个真实模型适配层，负责把统一请求映射到 Responses 风格接口，并已切到原生 async `reqwest` 主链
- `agent-store`：本地 SQLite session / trace 存储与查询
- `apps/agent-server`：最小应用壳，负责把共享运行时桥接到 HTTP + SSE
- `apps/web`：主界面承接层，消费服务端事件流并负责交互展示

## 模块边界

### Rust crate 内部组织约定

- 各 crate 的 `lib.rs` 保持为薄 façade，只负责 `mod` 声明与稳定 `pub use`
- 领域模型、协议映射、存储后端、兼容层、错误与测试分别落到独立模块，避免继续把实现堆在单个入口文件
- 内部模块化不改变 crate 级职责边界；跨 crate 的公开抽象仍以 `aia-config`、`agent-core`、`session-tape`、`agent-runtime`、`provider-registry`、`openai-adapter`、`agent-store` 的现有职责划分为准

### `aia-config`

负责应用级共享默认值与稳定约定：

- `.aia/` 目录及其下 `providers.json`、`session.jsonl`、`store.sqlite3`、`sessions/` 等默认路径
- server 默认 bind 地址、默认 base url、事件缓冲大小、请求超时等应用壳通用默认值
- 默认 session 标题、trace / span / prompt-cache 稳定前缀与统一 user agent 组装辅助
- 当前内部已拆为 `paths`、`server`、`identifiers` 三类模块，`lib.rs` 保持薄 façade
- 只承载共享配置，不承载 provider 业务、运行时编排、协议映射或算法阈值

### `agent-core`

负责纯领域抽象：

- 消息、角色、上下文窗口
- 模型能力与人格标签
- `LanguageModel` 已收口为单一流式入口：`complete_streaming(request, abort, sink)`；同步/非流式消费方通过空 sink 消费最终 `Completion`，避免 `complete` / `complete_streaming_with_abort` 三套入口长期并存
- 工具定义、工具调用、统一工具规范
- 运行时需要的请求与响应载荷
- 结构化会话条目：普通消息、工具调用、工具结果

### `session-tape`

负责可追溯会话：

- 扁平条目模型：每条 entry 均为 `{id, kind, payload, meta, date}`，对齐 republic / bub 数据模型
- `kind` 为字符串（message / system / anchor / tool_call / tool_result / event / error）
- `payload` 为 JSON Value，按 kind 语义承载类型化数据
- `meta` 为 JSON Value（对象），携带 run_id、source_entry_ids 等追踪信息
- `date` 为 ISO 8601 字符串，每条 entry 均带时间戳
- 工厂方法构造条目，builder 追加元数据（`with_meta` / `with_run_id`）
- 类型化访问器按需从 payload 反序列化（`as_message` / `as_tool_call` / `as_tool_result` / `anchor_name` / `event_name` 等）
- 轻量锚点：`Anchor {entry_id, name, state: Value}`，不再硬编码 phase / summary / next_steps / owner
- 命名锚点与按锚点切片查询
- handoff 时同时写入锚点与事件
- 默认从最新锚点之后重建上下文视图
- 工具调用与工具结果通过稳定调用标识关联
- 既保留面向通用展示的扁平消息投影，也保留面向模型请求重建的结构化会话投影
- 通过 jsonl 文件追加落盘可重放条目流，新格式直接序列化扁平 entry
- 追加式文件存储与内存存储都围绕“每个磁带一条独立日志”建模
- 分叉 / 合并只追加增量，不重写主线条目
- 旧格式兼容：载入时识别 `{id, fact, date}` 旧 JSONL 并自动转换为扁平条目，写出始终为新格式

### `agent-runtime`

负责把模型、工具、会话编排起来：

- 注册工具
- 追加用户输入
- 组装上下文视图与结构化模型请求，而不是只依赖扁平 `role/content`
- 调用模型
- 记录代理输出
- 通过统一事件方法向多个订阅者分发运行时事件
- 记录工具调用与工具结果到会话磁带
- 在执行前强校验工具是否可用，禁止绕过禁用策略
- 单个用户 turn 在运行时内部按多步循环执行：模型 → 工具调用 / 结果 → 再回模型，直到本轮不再产生工具调用或达到内部步数上限
- 多步循环上限不再是唯一硬编码常数；运行时保留默认安全护栏，并允许调用侧按场景覆盖
- 当多步循环到达最后预算步时，会切换到文本收尾模式并禁用工具，优先争取干净结束
- 工具不可用、执行失败、结果错配会被收敛为结构化失败调用结果并落入磁带，而不是立即中止整个会话
- 工具相关运行时事件直接携带结构化调用/结果载荷
- 整轮 turn 会进一步聚合为轮次块事件，便于客户端直接渲染时间线
- 历史轮次可从磁带 entries 按 `meta.run_id` 分组重建，不依赖磁带内 TurnRecord
- trace context 生成已统一通过共享 helper 收口，不再由不同路径各自手写 trace/span 标识
- stop/cancel 已贯穿 server → runtime → provider streaming / embedded shell
- `openai-adapter` 已改为原生 async `reqwest`：单次请求不再依赖 blocking client，流式读取改为 async chunk streaming + abort 轮询，避免 provider I/O 把后续 server 原生 async 化继续卡在边缘层
- 全异步主链已完成 Phase 1 / 2，并继续推进 Phase 3 / 4：`agent-core` 的模型/工具 trait、`agent-runtime` turn 主链、`openai-adapter` provider I/O 都已切到 async；`builtin-tools::shell` 已改为直接挂在 Tokio task 上的 async 事件泵，不再自建专用 thread/runtime，stdout/stderr 捕获也已改为异步 tail 临时 capture 文件，不再依赖 `spawn_blocking`；`read` / `write` / `edit` 已切到 `tokio::fs`，`glob` / `grep` 也已改为共享的 async `.gitignore` 感知仓库遍历 + async 文件读取，不再依赖 `spawn_blocking` / `ignore::WalkBuilder`，trace 查询路由也已去掉 per-request `spawn_blocking` 包装；当前剩余尾部主要是共享 SQLite store 的同步访问边界
- turn 主链内部已继续按职责拆为 `turn::{driver,segments,types}`：公开入口保持不变，流式 turn 驱动、completion segment 持久化与共享 turn buffer / success-failure context 分离，减少 runtime 单文件耦合与重复失败上下文拼装
- `turn::driver` 已继续清理历史样板：重复的失败收尾路径已收口为共享 `fail_turn` helper，避免取消/stop_reason/模型错误分支继续各自拼接 `record_turn_failure + return Err(...)`
- `agent-runtime` 对外 turn API 也已继续收口为单一异步入口 `handle_turn_streaming(user_input, control, sink)`：旧的同步 `handle_turn` 和历史命名 `handle_turn_streaming_with_control_async` 已移除，server 与测试消费方统一经由这条异步流式主链驱动 turn
- `agent-runtime` 的上下文压缩入口也已只保留异步 `auto_compress_now()`：旧的同步包装和内部 `block_on_sync` helper 已移除，避免 runtime 在共享层继续暴露“同步外壳 + 内部临时 runtime”模式
- `agent-runtime::runtime::tool_calls` 内部也已收口 runtime tool / 普通 tool 共用的生命周期记账路径：结果条目落盘、事件发布、`ToolInvocationLifecycle` 组装与 `seen_tool_calls` 更新不再在两条分支里各自复制，减少后续继续扩展工具语义时的分支漂移
- 时间辅助函数不假设系统时间恒定晚于 `UNIX_EPOCH`，异常场景下会安全回退
- `tape_info` / `tape_handoff` 已通过真正的 runtime tool registry 暴露，而不是字符串特判

当前内建编码工具契约维持短名集合：`shell`、`read`、`write`、`edit`、`glob`、`grep`。其中 `shell` 是模型可见的稳定工具名，底层执行器可在边缘实现中替换；当前实现使用 `brush` 作为 shell 运行时，而不是把具体 shell 名称泄漏进统一工具协议。

`builtin-tools::shell` 内部也已进一步按职责拆分：根模块只保留 `ShellTool` 契约与结果组装，capture 文件/事件泵与 embedded brush 执行主流程分别下沉到 `shell::{capture,execution}`，避免异步执行细节继续堆在单个超大文件里。

### `provider-registry`

负责本地 provider 管理：

- 保存 provider 档案
- 一个 provider 下可维护多个 `ModelConfig`，并记住 `active_model`
- 保存 provider 所属协议类型（Responses / OpenAI 兼容 Chat Completions）
- 记录当前活动 provider
- 从磁盘载入与写回 `.aia/providers.json`
- 兼容旧单模型落盘格式，并在载入 / 写入时把活动模型归一到有效 `ModelConfig`
- 兼容历史遗留 `.aia/sessions/providers.json` 回退读取
- 保持 provider 持久化逻辑不泄漏进应用壳层

### `openai-adapter`

负责首个真实模型提供商适配：

- 把内部统一请求映射为 Responses 风格 HTTP 请求
- 也支持映射为 OpenAI 兼容 Chat Completions 风格 HTTP 请求
- 单次请求与流式 SSE 已统一走 async `reqwest` 客户端，而不是 blocking client
- 流式读取使用 async chunk buffering + 行切分，并继续保留 abort 轮询与取消错误语义
- 把两类协议返回的文本、thinking 与工具调用统一还原为内部完成结果
- 在工具续接阶段按协议原生形态编码工具链路：Responses 使用 `function_call` / `function_call_output`，Chat Completions 使用 `assistant.tool_calls` / `tool.tool_call_id`
- 支持把共享层的 prompt cache 配置映射为 OpenAI `prompt_cache_key` / `prompt_cache_retention`
- 解析 Responses / Chat Completions usage 中的 `cached_tokens`，回填到共享 `CompletionUsage`
- 保持提供商细节停留在边缘层，不把外部协议泄漏进 `agent-core`
- `responses` 内部已进一步按职责拆分：根模块只保留配置与模型入口，请求构造/HTTP helper、响应体解析、流式状态累积与 `LanguageModel` 客户端入口分别下沉到 `responses::{request,parsing,streaming,client}`，避免边缘层协议映射、SSE 状态机与 HTTP 细节继续堆在单个超大文件里
- `chat_completions` 内部也已按相同模式拆分：根模块只保留配置与模型入口，请求构造/HTTP helper、响应体解析、流式状态累积与 `LanguageModel` 客户端入口分别下沉到 `chat_completions::{request,parsing,streaming,client}`，让两条 OpenAI 协议适配栈保持边界对称，便于后续继续收口共享 helper
- 两条协议共享的 HTTP/request helper 现已进一步收口到顶层共享模块：model 校验、HTTP client 构建、user-agent 注入、失败响应错误组装与 prompt-cache 请求体字段写入不再在 Responses / Chat Completions 两边各复制一份
- 协议专属 payload 类型也已拆回各自子模块：Responses 的反序列化载体位于 `responses::payloads`，Chat Completions 的反序列化载体位于 `chat_completions::payloads`；顶层不再保留跨协议混装的 `payloads.rs`，避免边缘层数据结构继续形成“共享垃圾桶”

### `agent-store`

负责本地 SQLite 存储：

- 持久化 session 列表与基础 session 元信息
- 持久化本地 trace/span 记录、聚合统计与查询结果
- trace store 内部已按 schema 初始化、store 查询/写入实现、row 映射与测试拆分子模块，避免 SQL、JSON 解码与提取 helper 继续堆在单个超大文件里
- 统一封装 `Mutex<Connection>` 访问，poisoned mutex 场景下可恢复 guard 继续服务
- `AiaStore` 现以 `with_conn(...)` 明确表达 SQLite 锁边界：session、trace、schema 初始化与 legacy 迁移都经由统一 helper 进入连接访问，避免各模块继续直接传播 `MutexGuard<Connection>`；这也为后续继续评估 store 边界是否需要再下沉或异步化留出单一入口
- 为 server 与 trace 诊断页提供本地存储支撑，而不把 SQLite 细节扩散到更多边界

### `apps/web`

负责主界面承接：

- 基于 React + Vite+ 构建 Web 工作台
- 只负责界面布局、交互与状态展示，不重写 agent loop 或工具编排
- 通过全局 SSE（`EventSource` → `GET /api/events`）消费结构化事件流
- 消息提交通过 `POST /api/turn` fire-and-forget，响应通过 SSE 返回
- 以前端全局 store 管理 SSE 连接、流式状态累积与 provider / session 状态刷新
- session 切换采用按 session 的本地 snapshot 缓存：切换时先显示已有快照并后台水合 history/current turn，减少消息区清空造成的闪烁与布局跳动
- 聊天消息区已做首轮渲染减载：turn 视图按引用 memo，长历史启用轻量窗口化渲染，并按 session 维持独立滚动位置，避免分页加载或切换会话时频繁强制跳到底部
- 聊天消息区窗口化已进一步升级为动态高度测量版；session 切换时明确回到底部看最新消息，而同一 session 内的历史分页仍保持当前阅读位置稳定
- 聊天消息区当前优先选择稳定渲染路径：已移除动态高度测量窗口化与锚定补偿，避免工具输出展开/收起或流式阶段因测量/补偿带来的额外抖动；列表性能继续依赖 memo、轻量历史首屏与后台补页控制
- session 切换首屏已改为“两阶段 hydrate”：切换前只同步保存旧 session 的最后一个 turn 快照；进入新 session 时先请求并展示最新一条历史 / 当前 turn，再后台补齐初始历史页，降低切换前后的主线程压力
- `_sessionSnapshots` 已收缩为最小 UI snapshot：仅保存最后一个 turn、`streamingTurn`、`chatState`、`contextPressure`、`lastCompression`，不再把历史页副本长期保留在前端内存中
- session 的后台补历史已改为空闲时增量补页，并支持在切走会话时中断，避免首屏切换后的非关键历史拉取继续和滚动/streaming 抢主线程与网络
- 空闲调度已收口为独立 helper：浏览器环境优先走 `requestIdleCallback`，不支持时回退 `setTimeout`，同时保留测试注入能力，避免调度策略散落在 store 内部
- 已覆盖 provider 管理、session 列表、历史消息、当前 turn 恢复、stop/cancel、trace 诊断视图
- 使用独立 Web 子目录规则，具体开发规范由 `docs/frontend-web-guidelines.md` 与 `apps/web/AGENTS.md` 约束

### `apps/agent-server`

负责 Web ↔ 运行时桥接：

- 基于 axum 构建 HTTP + SSE 服务器，监听端口 3434
- 启动时从 `.aia/providers.json`、`.aia/session.jsonl`、`.aia/store.sqlite3` 恢复本地状态
- 通过后台 runtime worker 独占 `AgentRuntime`、provider registry 与 session 落盘状态
- HTTP 路由已按 `provider`、`session`、`trace`、`turn` 领域模块拆分，共享错误响应、session 解析与 JSON helper 收口到 `routes::common`，避免 app 壳控制面继续堆在单个超大入口文件里
- `session_manager` 已进一步按职责拆成子模块：命令发送 handle、共享 slot/command 类型、current-turn 流式投影、tool trace 持久化与测试辅助分别独立，主文件只保留 session loop、slot 生命周期与 provider/runtime 同步主流程
- `model` 也已按职责拆成子模块：bootstrap mock、trace 事件收集/摘要/helper 与测试分别独立，主文件只保留 provider 选择、`ServerModel` 适配与 trace 落盘主流程
- `runtime_worker` 已按职责拆成子模块：共享类型、tape 快照重建/legacy decode helper 与测试分别独立，主文件只保留稳定 re-export 入口
- provider 当前信息、history 与 current turn 通过共享快照读取，避免长时间 turn 把所有路由一起锁住
- 全局 `broadcast::channel` 向所有 SSE 客户端推送事件
- 暴露 provider、session、turn、cancel、handoff、trace 等 HTTP API
- `POST /api/turn` 仍保持 fire-and-forget，但真正的 turn 执行、事件回收与 session 条目追加都在 worker 内串行完成
- turn 执行与 session manager 已切到原生 Tokio async task：`apps/agent-server` 不再依赖 `tokio::spawn_blocking`、`std::thread::Builder`、`LocalSet` 或 `spawn_local` 承载 turn 主链；worker 直接 await `AgentRuntime::handle_turn_streaming(...)`，压缩路径也直接 await `AgentRuntime::auto_compress_now()`，运行中 `session/info` 通过 slot 内的 `ContextStats` 快照读取 live stats，turn 结束后仍沿用显式 runtime ownership 归还路径
- 运行中的条目会实时 append 到 `.aia/session.jsonl`
- provider 变更采用事务式提交，避免 registry / runtime / tape 持久化分叉
- 启动失败与 JSON 序列化失败都已收口为结构化错误路径，而不是 panic
- 不含 agent loop 逻辑，纯粹作为运行时的 HTTP 外壳

## 下一阶段直接承接的能力

- MCP 客户端 / 服务端桥接
- 统一工具规范向 Claude / Codex / MCP 的外部映射
- Web 界面会话恢复与更细粒度的 provider / session 状态管理继续完善
- runtime 驱动辅助继续从 `apps/agent-server` 上移到共享层
- 桌面壳复用当前 Web 前端与 Rust 核心
- trace 资源语义 / richer events / exporter 后续增强
