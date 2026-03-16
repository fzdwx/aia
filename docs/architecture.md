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
- `llm-trace`：本地 trace 存储与查询层，负责保留 LLM 请求记录并为 Web 诊断页提供 loop / span 视角
- `apps/agent-server`：最小应用壳，负责把共享运行时桥接到 HTTP + SSE
- `apps/web`：主界面承接层，消费服务端事件流并负责交互展示

## 模块边界

### Rust crate 内部组织约定

- 各 crate 的 `lib.rs` 保持为薄 façade，只负责 `mod` 声明与稳定 `pub use`
- 领域模型、协议映射、存储后端、兼容层、错误与测试分别落到独立模块，避免继续把实现堆在单个入口文件
- 内部模块化不改变 crate 级职责边界；跨 crate 的公开抽象仍以 `aia-config`、`agent-core`、`session-tape`、`agent-runtime`、`provider-registry`、`openai-adapter` 的现有职责划分为准

### `aia-config`

负责应用级共享默认值与稳定约定：

- `.aia/` 目录及其下 `providers.json`、`session.jsonl`、`store.sqlite3`、`sessions/` 等默认路径
- server 默认 bind 地址、默认 base url、事件缓冲大小、请求超时等应用壳通用默认值
- 默认 session 标题与统一 user agent 组装辅助
- 只承载共享配置，不承载 provider 业务、运行时编排或协议映射

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
- 追加式文件存储与内存存储都围绕”每个磁带一条独立日志”建模
- 分叉 / 合并只追加增量，不重写主线条目
- 旧格式兼容：载入时识别 `{id, fact, date}` 旧 JSONL 并自动转换为扁平条目，写出始终为新格式

这部分会成为后续”增量压缩”和”fork / handoff”的基础。

### `agent-runtime`

负责把模型、工具、会话编排起来：

- 注册工具
- 追加用户输入
- 组装上下文视图
- 组装结构化模型请求上下文，而不是只依赖扁平 `role/content` 消息
- 调用模型
- 记录代理输出
- 通过统一事件方法向多个订阅者分发运行时事件
- 记录工具调用与工具结果到会话磁带（通过 `TapeEntry` 工厂方法 + `with_run_id` 追踪）
- 在执行前强校验工具是否可用，禁止绕过禁用策略
- 单个用户 turn 在运行时内部按多步循环执行：模型 → 工具调用 / 结果 → 再回模型，直到本轮不再产生工具调用或达到内部步数上限
- 多步循环上限不再是唯一硬编码常数；运行时保留默认安全护栏，并允许调用侧按场景覆盖
- 当多步循环到达最后预算步时，运行时会切换到文本收尾模式并禁用工具，优先争取干净结束，而不是立刻以步数错误中止
- 工具不可用、执行失败、结果错配会被收敛为结构化失败调用结果并落入磁带，供同轮后续模型步骤继续消费，而不是立即中止整个会话
- 工具相关运行时事件直接携带结构化调用/结果载荷，而不是仅传字符串
- 工具相关运行时事件进一步聚合为单个调用生命周期块，便于客户端直接渲染
- 整轮 turn 进一步聚合为轮次块事件，便于界面直接渲染完整时间线
- 整轮 turn 现在保留按发生顺序排列的块级序列（thinking / assistant / tool / failure），界面层不再只能依赖聚合字段猜测顺序
- 每个轮次块带稳定轮次标识与时间戳，便于时间线跳转与回放定位
- 轮次块只作为运行时事件发出，不再写入磁带（"derivatives never replace original facts"）
- 历史轮次可从磁带 entries 按 `meta.run_id` 分组重建，不依赖磁带内的 TurnRecord
- crate 内部已继续拆分为运行时主循环、请求构造、工具执行、事件缓冲、错误与测试子模块，避免继续把所有逻辑堆在单个实现文件中
- `apps/agent-server` 现在提供显式 turn cancel API；session manager 为运行中轮次持有 `TurnControl`，Web/其他客户端可发起取消，runtime 会把 abort signal 继续传到工具执行上下文与模型流式调用；当前 OpenAI streaming 读取会在 SSE 循环中主动检查取消信号，embedded shell 也会尝试向 brush 当前作业发送 `TERM` 收尾，取消完成态则通过共享 `TurnLifecycle.outcome` 明确表达，避免前后端继续仅靠 failure message 猜测；server 侧也把“请求取消”与“轮次真正结束”为 cancelled 的 SSE 发射点收口为单一完成路径，避免客户端收到重复 cancelled 事件；块级结果里取消也使用独立 `TurnBlock::Cancelled`，不再伪装成 failure
- `agent-store` 的 SQLite 访问统一通过可恢复的 `Mutex<Connection>` guard 辅助方法进入；即使之前有持锁 panic 造成 poisoned mutex，trace/session 读写与 schema 初始化也不会再直接 panic，而是恢复 guard 继续提供本地存储能力
- `apps/agent-server` 的进程启动初始化路径也遵循同样原则：provider registry、统一 store、sessions 目录、默认 session、模型构建、监听端口与 `axum::serve` 失败都收口为结构化初始化错误，不再在主入口用 `expect` 直接 panic
- `apps/agent-server` 路由层的 JSON 响应序列化同样不依赖 `expect`；session/trace/current-turn/info 等 handler 统一通过安全序列化 helper 生成响应，避免“本应返回 500 的序列化失败”被升级成服务 panic
- `agent-core` 与 `agent-runtime` 的时间辅助函数不假设系统时间恒定晚于 `UNIX_EPOCH`；tool invocation id、turn id 与运行时时间戳在时钟回拨场景下会安全回退为零基线，避免因宿主时间异常触发 panic
- `builtin-tools` 的 shell 测试基线不假设 stdout/stderr 流一定只产生单个 delta；验证聚焦于最终拼接后的流内容，减少嵌入式 shell 线程调度带来的脆弱回归
- `StreamEvent` 中与工具相关的语义继续细分：`ToolCallDetected` 表示模型流里已经产出 tool call 决策，但 runtime 还未真正开始执行；`ToolCallStarted` 才表示工具执行正式启动，避免把“模型建议”与“runtime 执行”混成同一个阶段
- `tape_info` / `tape_handoff` 不再只是 `execute_tool_call` 里的字符串特判；它们现在通过 `Tool` trait 注册到独立 runtime tool registry，再借助 `ToolExecutionContext` 暴露的 runtime host 能力访问会话统计与 handoff 写入，工具协议层与普通工具保持一致

当前内建编码工具契约维持短名集合：`shell`、`read`、`write`、`edit`、`glob`、`grep`。其中 `shell` 是模型可见的稳定工具名，底层执行器可在边缘实现中替换；当前实现使用 `brush` 作为 shell 运行时，而不是把具体 shell 名称泄漏进统一工具协议。

provider 变更路径也已收口为事务式提交：候选 registry 必须先通过模型构建校验，随后 `providers.json` 与 `session.jsonl` 落盘都成功，才会更新内存中的 registry、runtime 与 tape，避免出现重启前后 provider 绑定分叉。

第一阶段只实现最小 turn 执行，故意不把并发子代理一次做满，避免空壳化。

### `provider-registry`

负责本地 provider 管理：

- 保存 provider 档案
- 一个 provider 下可维护多个 `ModelConfig`，并记住 `active_model`
- 保存 provider 所属协议类型（Responses / OpenAI 兼容 Chat Completions）
- 记录当前活动 provider
- 从磁盘载入与写回 `.aia/providers.json`
- 兼容旧单模型落盘格式，并在载入 / 写入时把活动模型归一到有效 `ModelConfig`
- 保持 provider 持久化逻辑不泄漏进应用壳层

### `openai-adapter`

负责首个真实模型提供商适配：

- 把内部统一请求映射为 Responses 风格 HTTP 请求
- 也支持映射为 OpenAI 兼容 Chat Completions 风格 HTTP 请求
- 把两类协议返回的文本、thinking 与工具调用统一还原为内部完成结果
- 在工具续接阶段按协议原生形态编码工具链路：Responses 使用 `function_call` / `function_call_output`，Chat Completions 使用 `assistant.tool_calls` / `tool.tool_call_id`
- 支持把共享层的 prompt cache 配置映射为 OpenAI `prompt_cache_key` / `prompt_cache_retention`，当前由 server 自动提供 session 级 key 和固定 `24h` retention
- 解析 Responses / Chat Completions usage 中的 `cached_tokens`，回填到共享 `CompletionUsage`
- 保持提供商细节停留在边缘层，不把外部协议泄漏进 `agent-core`

### `llm-trace`

负责本地 tracing 诊断数据：

- 基于 SQLite 持久化每次 LLM 请求的记录、列表摘要与聚合统计
- 记录 provider、protocol、request/response 摘要、耗时、token、cached token、HTTP 状态码与错误信息
- 当前已为每条 LLM 请求补上稳定本地 `trace_id` / `span_id` / `parent_span_id` / `root_span_id`、`span_kind`、`operation_name`、`otel_attributes` 与 `events`
- 当前存储实体本质上已从“只有 LLM request record”推进到“本地 span store”：LLM 请求会落成 CLIENT spans，runtime 工具执行也会落成 INTERNAL spans，并共用同一 trace/root span 语义
- 现阶段仍不是完整 OpenTelemetry span/event 存储：没有 OTLP exporter、collector 管线、资源属性与跨服务传播
- Web trace 页会把同一 `turn_id` / `run_id` 组合解释为一个 agent loop，并把其中的每个 LLM 请求映射为 child span
- runtime 工具调用生命周期会在 server 侧直接落成 tool spans，用来还原“LLM -> tool -> LLM”的执行路径；界面层优先消费真实 span 记录，而不是再自行推导
- 后续若接 OTLP / collector，应继续在这里补 span events、links、资源属性与 exporter，而不是把 exporter 逻辑塞进 `apps/agent-server`

### `apps/web`

负责主界面承接：

- 基于 React + Vite 构建 Web 工作台
- 优先使用 `shadcn` 基础组件承接卡片、输入、滚动容器与状态徽标
- 只负责界面布局、交互与状态展示，不重写 agent loop 或工具编排
- 通过全局 SSE（`EventSource` → `GET /api/events`）消费结构化事件流
- 消息提交通过 `POST /api/turn` fire-and-forget，响应通过 SSE 返回
- 以前端全局 store 管理 SSE 连接、流式状态累积与 provider 当前状态 / 列表刷新
- 流式 turn 实时渲染 thinking / tool output / assistant text，并显示状态阶段指示器；Markdown 默认由前端直接渲染
- trace 页优先展示 loop root、LLM spans 与 tool spans 的关系，而不是直接暴露原始请求日志列表；原始 payload 退为下钻视图，选中节点后可进一步查看本地 event timeline
- 具体开发规范由 `docs/frontend-web-guidelines.md` 约束

### `apps/agent-server`

负责 Web ↔ 运行时桥接：

- 基于 axum 构建 HTTP + SSE 服务器，监听端口 3434
- 启动时从 `.aia/providers.json` 与 `.aia/session.jsonl` 恢复 provider 绑定，无匹配时回退 bootstrap
- 通过后台 runtime worker 独占 `AgentRuntime`、provider registry 与 session 落盘状态，HTTP 路由仅在提交 turn、handoff、provider 变更等写操作时通过消息传递访问运行时
- provider 当前信息、已完成 history 与运行中 current turn 通过共享快照读取，避免长时间 turn 把所有路由一起锁住
- 全局 `broadcast::channel` 向所有 SSE 客户端推送事件
- HTTP API：
- `GET /api/providers`：返回当前 provider 信息
- `GET /api/providers/list`：返回 provider 列表与活动项信息
- `GET /api/session/history`：返回已完成的 `TurnLifecycle[]`
- `GET /api/session/current-turn`：返回当前运行中的 turn 快照（如果存在）
- `GET /api/events`：全局 SSE 事件流（stream / status / turn_completed / error）
- `POST /api/turn`：发送用户消息（202 fire-and-forget）
- `POST /api/providers` / `PUT /api/providers/:name` / `DELETE /api/providers/:name` / `POST /api/providers/switch`：provider 管理与切换
- 从 `StreamEvent` 变体自动派生轮次状态（ThinkingDelta → Thinking，TextDelta → Generating，ToolCallStarted / ToolOutputDelta → Working）；`ToolCallDetected` 只用于展示待执行工具，不应把轮次提前标记为 Working
- `POST /api/turn` 仍保持 fire-and-forget，但真正的 turn 执行、事件回收与 session 条目追加都在 worker 内串行完成
- 运行中的条目会实时 append 到 `.aia/session.jsonl`，history / current turn 则通过共享快照立即可读
- provider 变更采用事务式提交，避免 registry / runtime / tape 持久化分叉
- 不含 agent loop 逻辑，纯粹作为运行时的 HTTP 外壳

## 对 README 的映射

### 已覆盖的第一阶段能力

- 代理核心结构
- 工具协议统一入口
- 会话磁带、结构化锚点、工具事实与 handoff 事件基础
- 模型人格元信息
- 工具按名称启停的基础能力
- provider 本地注册与活动项选择基线
- 首个真实模型适配器基线
- 最小可运行的多轮 agent loop
- 可被多个订阅者消费的统一事件流基线
- 最小可运行的 HTTP + SSE 应用壳与 Web 主界面骨架

### 下一阶段直接承接的能力

- MCP 客户端 / 服务端桥接
- 统一工具规范向 Claude / Codex / MCP 的外部映射
- Web 界面会话恢复与更细粒度的 provider / session 状态管理
- 子代理调度
- 桌面壳
- 压缩策略与 fork

## 技术方向

第一阶段选用 Rust 工作区，原因：

- Web 客户端与运行时都偏性能敏感
- 跨平台二进制分发稳定
- 后续接桌面壳时可以直接复用 Rust 核心

桌面层建议后续采用 Rust 核心 + Web 前端壳的方式，这样可以保留轻量分发与较低内存占用。

## 协议方向

- **对内**：维护一套统一工具规范，避免被单一模型厂商绑死
- **对外**：优先对接 MCP 这一类标准工具协议
- **适配层**：后续为不同模型工具规范增加边缘映射，而不是污染核心运行时

## Trace / OTel 方向

当前 trace 方案的定位是“OTel-shaped local diagnostics”，而不是“已经完整接入 OpenTelemetry”：

- 界面层已按 root span / child spans / internal tool spans 的方式组织信息，便于和 TraceLoop、Phoenix、Langfuse 这类产品的阅读路径靠拢
- 领域边界上，agent loop、LLM request、tool execution 的区分已经收口，可继续向标准 span 模型演进
- 当前已经具备稳定本地 span 标识、基础父子链路与一份 otel-style attributes，但仍缺完整事件列表、资源语义约束与 OTLP 导出
- 因此现阶段重点是先把本地 trace 语义和展示做对，再在 `llm-trace` / adapter 边界补 exporter，而不是过早把 collector 细节扩散到运行时和界面层
