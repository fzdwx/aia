# 项目状态

## 当前阶段

- 阶段：核心工作区搭建之后的当前细分步骤：Web 界面 ↔ 运行时桥接收口
- 当前步骤：在 Web + server 主路径稳定的基础上，继续收口“可作为其他客户端驱动接口”的 server 形态，并把 trace 诊断链路从“LLM 请求日志 + 前端推导 tool 节点”推进到“后端真实 span 持久化 + 前端直接消费真实 span”视角；当前已补上 OpenAI prompt caching 的统一接入：server 自动为每个 session 生成稳定 cache key，固定 `24h` retention，并把 `cached_tokens` 打通到 usage / trace / Web 展示

## 已完成

- 建立 Rust 工作区
- 建立 `agent-core`
- 建立 `session-tape`
- 建立 `agent-runtime`
- 建立 `provider-registry`
- 建立 `openai-adapter`
- 建立 `apps/web` Web 工程骨架
- 完成最小可运行验证与基础测试覆盖
- 完成项目级命名从 `like` 收敛为 `aia`
- 完成本地 provider 注册、活动项持久化与协议类型区分
- 完成 OpenAI Responses 与 OpenAI 兼容 Chat Completions 双协议适配
- 完成会话磁带扁平化、锚点、handoff、fork / merge、查询切片与旧格式兼容
- 完成结构化工具调用 / 工具结果贯穿运行时主链，并统一沉淀为可重建请求的会话条目
- 完成 OpenAI Responses 与 Chat Completions 的原生工具链路映射
- 完成运行时单轮多步模型 / 工具循环、重复工具调用防重、预算提示、文本收尾步与独立工具调用上限
- 完成 `apps/web` 首页从模板页替换为项目主界面骨架
- 完成 `apps/web` 工作台首页重构，并接入 `shadcn` 基础组件体系（card、badge、input、textarea、separator、scroll-area）
- 完成 Web 主界面信息结构收敛：左侧边栏、中央消息列表、底部输入框，去掉发散型展示布局
- 完成 `docs/frontend-web-guidelines.md`，明确 Web 前端开发规范与运行时边界
- 完成 `apps/agent-server` axum HTTP+SSE 服务器，桥接 Web 前端到共享运行时
- 完成全局 SSE 事件流架构（`GET /api/events`），基于 `broadcast::channel` 向所有客户端推送事件
- 完成 `POST /api/turn` fire-and-forget 消息提交，响应通过全局 SSE 返回
- 完成 `GET /api/providers`、`GET /api/session/history` 与 `GET /api/session/current-turn` 数据接口
- 完成 Rust 侧核心类型（StreamEvent、TurnLifecycle、TurnBlock 等）的 Serialize/Deserialize 支持
- 完成前端 TypeScript 类型定义镜像 Rust 侧类型（discriminated union 对齐 serde tag）
- 完成前端全局 store：统一管理 EventSource 连接、流式状态累积、turn 完成回收与 provider 当前状态刷新
- 完成流式轮次状态指示：waiting → thinking → working → generating，shimmer 文字动画
- 完成流式 tool_output_delta 实时渲染，按 invocation_id 分组展示，不等 turn_completed
- 完成 Vite 开发代理配置（`/api` → `http://localhost:3434`）
- 完成 justfile 开发命令（`just dev` 同时启动前后端）
- 完成移除 `apps/agent-cli` 包，并同步清理工作区与文档中的 CLI 主入口叙事
- 完成核心 Rust crates 的内部模块化收口：`provider-registry`、`agent-core`、`session-tape`、`openai-adapter`、`agent-runtime` 已从单文件主入口拆为薄 `lib.rs` + 职责模块
- 完成 `provider-registry` 与 `apps/agent-server` 之间的 provider 多模型配置接口对齐，并兼容旧单模型本地落盘格式
- 完成 `agent-runtime` 深一层内部拆分：主循环、请求构造、工具执行、事件缓冲、错误与测试已进一步解耦
- 完成内建编码工具命名收口为 `shell`、`read`、`write`、`edit`、`glob`、`grep`
- 完成 `shell` 内建工具改为内嵌 `brush` 库执行，并补齐 stdout / stderr / exit_code 结构化回传与基础测试
- 完成 `apps/agent-server` 向 runtime 显式传入 `workspace_root`，保证相对路径工具调用语义稳定
- 完成 Web 端 provider 创建、更新、删除、切换与当前 provider / provider 列表刷新链路
- 完成 provider 变更的事务式提交：候选 registry 校验、registry 落盘、session tape 落盘全部成功后才提交到内存 runtime / tape
- 完成 provider 持久化失败路径回归测试，保证落盘失败不会留下 registry / runtime / tape 分叉状态
- 完成 Web 端 Markdown 渲染入口收敛为共享前端组件，并参考 opencode 的消息排版规则统一标题、列表、引用、表格与代码块样式
- 完成 `apps/agent-server` 运行时拥有关系重构：当前由后台 runtime worker 独占 `AgentRuntime`、provider registry 与 session 持久化，HTTP 路由通过消息传递访问运行时，不再用全局 Mutex 包住整个 turn
- 完成 provider 当前信息 / provider 列表快照化读取，长时间 shell / model turn 不再阻塞轻量查询接口
- 完成 session history / current-turn 快照化读取：运行中的 agent loop 不再把 `/api/session/history` 挂起，页面刷新时也能直接恢复当前进行中的 turn
- 完成 session jsonl 实时 append 落盘：agent loop 过程中新增的用户消息、thinking、tool 调用结果与完成/失败事件都会立即写入 `.aia/session.jsonl`
- 完成 Web 端用户消息的乐观渲染，提交后立即显示到消息列表，而不是等流式完成再落入 completed turn
- 完成 Web 端 trace 详情模态框的诊断视图重构：第一行先展示 trace overview，下方分离执行结果与请求上下文，右侧集中放消息时间线与 tool schema，原始 payload 退为补充折叠区
- 完成 trace 记录对真实 HTTP 状态码的保留：不再在成功路径硬编码 `200`，OpenAI 适配器错误也会把上游状态码与响应体带回 trace
- 完成 trace 列表按 agent loop 聚合展示：列表先显示同一轮的 LLM 调用次数、总耗时、总 token 与 stop reason 路径，展开后再查看每个 step
- 完成 trace loop 列表项直接基于 trace payload 提炼并展示本轮用户消息预览，不再依赖 chat store 关联，进入 trace 页面即可先判断这一轮在处理什么，再决定是否展开 step 明细
- 完成对 `openai-responses` provider 私有续接链路的移除，统一回退为发送完整结构化上下文，规避兼容端在续接路径上的 5xx 失败
- 完成共享协议层与 trace/UI 对 provider 私有 checkpoint 概念的彻底移除：`Completion` / `CompletionRequest` / `session-tape` / trace API / trace 页面不再暴露或持久化该能力
- 完成 Web trace 页从“loop 列表 + 详情模态框”为主，收口为更接近 tracing 产品的三栏视图：左侧 recent loops，中间 span timeline，右侧 inspector；同一 agent loop 被解释为 root span，单次 LLM 请求解释为 CLIENT span，runtime tool 生命周期解释为 INTERNAL span
- 完成 `llm-trace` 在架构文档与 README 中的职责补齐，并明确当前 trace 仍是本地 OTel-shaped 诊断模型，而不是完整 OpenTelemetry / OTLP exporter
- 完成 `llm-trace` 持久化字段升级：为每条 LLM 请求补上稳定本地 `trace_id` / `span_id` / `parent_span_id` / `root_span_id`、`span_kind`、`operation_name` 与 `otel_attributes`，并补齐 SQLite migration，现有 trace 库无需重建
- 完成共享 trace context 生成逻辑收口：runtime 请求构造与 context compression 不再各自手写 trace 标识，而是统一生成 loop trace id、root span id 与 request span id
- 完成 `llm-trace` 本地 event timeline 落盘：记录 request started、首个 reasoning/text delta、模型建议 tool call、response completed/failed，并在 trace 页面 inspector 中展示所选 root/llm/tool 节点的事件时间线
- 完成 runtime tool span 的后端真实落盘：工具执行不再只是前端基于 turn history 的临时推导节点，而是会继承当前 loop trace/root span 语义，在 `apps/agent-server` 中直接持久化为 INTERNAL span record，并带本地 tool started/completed/failed events
- 完成 Web trace 页改为优先消费真实 tool span 记录：timeline / inspector 选中工具节点时会直接查看该工具 span 的 request/response/attributes/events，而不是回退查看父 LLM span
- 完成流式工具事件语义拆分：模型侧只发 `tool_call_detected` 表示“已经决定要调工具”，runtime 真正开始执行时才发 `tool_call_started`，避免前端把一次工具调用误看成两次 start
- 完成 `tape_info` / `tape_handoff` 从 runtime 特判式实现收口到 `Tool` trait + runtime tool registry：schema 暴露与调用路径改为真正的工具注册模型，只通过 `ToolExecutionContext` 注入最小 runtime host 能力
- 完成 trace loop 列表后端分页：`/api/traces` 改为按 loop（`trace_id`）分页返回当前页 span 集合与总 loop 数，Web 端 recent loops 不再在单页内前端切片 12 条，而是直接消费后端页信息
- 完成真实 token usage 贯通到 turn 主链：provider 返回的 `completion.usage` 现在会进入 `TurnLifecycle`、随 `turn_completed` SSE 与 session history 一起返回，并持久化到 `turn_completed` tape event，Web 聊天视图可直接显示本轮 input/output/total tokens
- 完成自动上下文压缩触发修正：runtime 现在不仅会在下一轮开始前依据上一轮真实 `input_tokens` 预判压缩，也会在当前轮成功结束并拿到新的真实 usage 后立即补做一次压缩；同时修复“provider 本轮未返回 usage 时沿用旧 token 统计”导致的误判，避免跨轮错误触发或漏触发压缩
- 完成上下文压缩可观测性补齐：`tape_info` 现在返回结构化 JSON 内容并附带 tool result details，前端会显式消费 `context_compressed` SSE 并展示最近一次压缩摘要，同时继续刷新 session context pressure
- 完成提交前的后端自动压缩收口：`POST /api/turn` 现在会先读取 session context stats，并在压力达到自动压缩阈值时先通过 session manager 触发 idle auto-compress，再真正启动本轮 turn；runtime 内部预压缩顺序也已调整为“先压缩旧上下文，再追加新用户消息”
- 完成 Web 历史消息体验优化：切换 session / 水合历史时直接跳到底部，实时新增内容仅在用户仍接近底部时才自动跟随；`/api/session/history` 同时收口为按 turn 的分页接口，前端首次只加载最新一页并支持按需继续拉取更早历史，避免长会话进入时慢速滚动与一次性渲染过多消息
- 完成 Web 端 turn 提交请求的 `keepalive` 加固：页面刷新或跳转时，已发出的 `POST /api/turn` 不再容易因为浏览器中断请求而导致本轮根本未进入 server worker
- 完成 provider 注册表加载的旧路径兼容：当 `.aia/providers.json` 缺失时，server 会自动回退读取历史遗留的 `.aia/sessions/providers.json`，避免已有 provider 数据因为路径迁移而在启动后表现为“空配置”
- 完成完整的 stop/cancel 基线：server 暴露 `POST /api/turn/cancel`，session manager 能中断运行中 turn，runtime 把取消信号传到工具执行上下文，Web 输入区提供 stop 按钮并显示 cancelled 状态
- 完成 stop/cancel 第二阶段基线：runtime 会把 abort 继续传到 OpenAI streaming 调用，`openai-adapter` 在流式读取中主动检查取消信号；embedded `brush` shell 在收到取消后会向当前作业发送 `TERM` 并尽快收尾；`TurnLifecycle` 新增共享 `outcome` 字段，让前后端不再仅靠 `failure_message` 猜测取消状态；server 取消 API 只负责触发 abort，真正的 cancelled SSE 改由 worker 在轮次结束时统一发出一次，避免重复事件；块级协议也已把取消从 `failure` 中拆出为独立 `cancelled` block，避免前端继续把取消渲染成失败
- 完成 `agent-store` SQLite 锁中毒恢复：trace/session 读写与 schema 初始化不再因 `Mutex<Connection>` poisoned 而 panic，改为恢复 guard 继续服务，并补回归测试覆盖 poisoned mutex 路径
- 完成 `aia-config` 共享配置 crate：把 `.aia` 路径、默认 session 标题、server 默认地址 / 事件缓冲 / 请求超时、统一 user agent 组装从 `apps/agent-server` 与存储相关 crate 中收口，减少公共常量散落
- 完成 `provider-registry`、`session-tape`、`apps/agent-server` 对共享配置默认值的首轮接入
- 完成 `apps/agent-server` 启动路径错误收口：provider 注册表、SQLite store、sessions 目录、默认 session、模型构建、端口绑定与 server serve 失败不再 `expect` panic，而是统一返回结构化初始化错误并以非零退出码结束进程
- 完成 `apps/agent-server` 路由响应序列化收口：session/trace/current-turn/info 等 JSON 响应不再因 `serde_json::to_value(...).expect(...)` 在服务路径 panic，而是统一回退为 500 错误响应
- 完成 `agent-core` / `agent-runtime` 时间辅助函数收口：tool invocation id、turn id 与时间戳生成在系统时钟回拨到 UNIX_EPOCH 之前时不再 panic，而是安全回退为零时长基线
- 完成 `builtin-tools` shell 测试稳定性修正：stdout delta 断言不再假设嵌入式 shell 只会回传单个输出块，避免 full-suite 下的脆弱测试失败

## 正在进行

- 收口 runtime worker 留在 `apps/agent-server`、哪些能力适合上移到 `agent-runtime` 的边界
- 观察内嵌 `brush` 作为 shell 运行时的实际稳定性、命令兼容性与中断语义
- 继续把 trace 数据模型从“本地 span store + event timeline”推进到更完整的 resources / richer events 模型，但暂不抢在工具协议映射与 MCP 之前做 exporter / collector 集成
- 验证 stop/cancel 目前对长时间 shell / 外部 provider streaming 的实际覆盖率；当前已打通 server→runtime→tool context，并进一步补上 OpenAI streaming 读取中的取消检查与 shell 作业 `TERM` 中断，后续仍需继续观察 provider/运行时在不同上游和复杂 shell pipeline 下的真实中断覆盖率

## 下一步

1. 继续观察并补强 stop/cancel 在不同 OpenAI 兼容上游与复杂 embedded shell pipeline 下的实际中断覆盖率，必要时把“读流中断”继续升级为更底层的 HTTP 连接级取消
1. runtime 驱动辅助从 `apps/agent-server` 继续抽到共享层
2. 在工具协议边界进一步收稳后，把 `llm-trace` 从当前本地 span record + event timeline 继续推进到更完整的 resources / richer events 形态
3. 继续补强 shell 中断 / 长任务处理与更细粒度的工具运行时能力
4. 桌面壳接入


## 暂时不做

1. 继续推进统一工具规范向外部协议映射与 MCP 接入

## 为什么当前先做 Web，而不是继续堆终端界面

因为共享运行时、会话模型和工具协议主链已经稳定，继续维护独立终端壳只会增加重复界面成本。当前更合理的方向是让 `apps/web` 直接承接主界面，再由桌面壳复用同一 Web 前端与 Rust 核心；而在主界面主路径已经收口后，下一优先级应回到统一工具规范的外部映射与 MCP 接入，而不是继续提前堆厚更多客户端表层能力。

当前 trace 观测性也遵循同样原则：先把共享语义边界收稳，再谈 exporter 和外部 tracing 平台对接；如果工具协议和运行时事件边界还没完全稳定，就过早绑定某个 tracing 后端，只会让后续协议演进成本更高。

## 阻塞

- 当前无硬阻塞；已知非阻断事项主要是前端生产包体积提示偏大，以及 `shell` 的中断能力与长任务取消语义仍可继续增强
