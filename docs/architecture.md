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
- `openai-adapter`：首个真实模型适配层，负责把统一请求映射到 Responses 风格接口
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
- `openai-adapter` 的流式读取已从单线程阻塞 `BufRead::lines()` 轮询升级为“后台按行泵送 + 前台 abort 轮询”，避免 SSE 长时间不出新行时取消被卡在阻塞读里
- 时间辅助函数不假设系统时间恒定晚于 `UNIX_EPOCH`，异常场景下会安全回退
- `tape_info` / `tape_handoff` 已通过真正的 runtime tool registry 暴露，而不是字符串特判

当前内建编码工具契约维持短名集合：`shell`、`read`、`write`、`edit`、`glob`、`grep`。其中 `shell` 是模型可见的稳定工具名，底层执行器可在边缘实现中替换；当前实现使用 `brush` 作为 shell 运行时，而不是把具体 shell 名称泄漏进统一工具协议。

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
- 把两类协议返回的文本、thinking 与工具调用统一还原为内部完成结果
- 在工具续接阶段按协议原生形态编码工具链路：Responses 使用 `function_call` / `function_call_output`，Chat Completions 使用 `assistant.tool_calls` / `tool.tool_call_id`
- 支持把共享层的 prompt cache 配置映射为 OpenAI `prompt_cache_key` / `prompt_cache_retention`
- 解析 Responses / Chat Completions usage 中的 `cached_tokens`，回填到共享 `CompletionUsage`
- 保持提供商细节停留在边缘层，不把外部协议泄漏进 `agent-core`

### `agent-store`

负责本地 SQLite 存储：

- 持久化 session 列表与基础 session 元信息
- 持久化本地 trace/span 记录、聚合统计与查询结果
- 统一封装 `Mutex<Connection>` 访问，poisoned mutex 场景下可恢复 guard 继续服务
- 为 server 与 trace 诊断页提供本地存储支撑，而不把 SQLite 细节扩散到更多边界

### `apps/web`

负责主界面承接：

- 基于 React + Vite+ 构建 Web 工作台
- 只负责界面布局、交互与状态展示，不重写 agent loop 或工具编排
- 通过全局 SSE（`EventSource` → `GET /api/events`）消费结构化事件流
- 消息提交通过 `POST /api/turn` fire-and-forget，响应通过 SSE 返回
- 以前端全局 store 管理 SSE 连接、流式状态累积与 provider / session 状态刷新
- 已覆盖 provider 管理、session 列表、历史消息、当前 turn 恢复、stop/cancel、trace 诊断视图
- 使用独立 Web 子目录规则，具体开发规范由 `docs/frontend-web-guidelines.md` 与 `apps/web/AGENTS.md` 约束

### `apps/agent-server`

负责 Web ↔ 运行时桥接：

- 基于 axum 构建 HTTP + SSE 服务器，监听端口 3434
- 启动时从 `.aia/providers.json`、`.aia/session.jsonl`、`.aia/store.sqlite3` 恢复本地状态
- 通过后台 runtime worker 独占 `AgentRuntime`、provider registry 与 session 落盘状态
- provider 当前信息、history 与 current turn 通过共享快照读取，避免长时间 turn 把所有路由一起锁住
- 全局 `broadcast::channel` 向所有 SSE 客户端推送事件
- 暴露 provider、session、turn、cancel、handoff、trace 等 HTTP API
- `POST /api/turn` 仍保持 fire-and-forget，但真正的 turn 执行、事件回收与 session 条目追加都在 worker 内串行完成
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
