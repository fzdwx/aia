# 项目状态

## 当前阶段

- 阶段：核心工作区搭建之后的当前细分步骤：Web 界面 ↔ 运行时桥接收口
- 最新前端进展：`apps/web` 已补齐与 `Settings` 平行的 `Channels` 配置页，当前支持飞书 channel 的列表、创建、编辑、删除与启停；入口已接入侧边栏 `Trace/Channels/Settings` 同组导航，前端 store 与 `/api/channels` CRUD 调用链也已打通。
- 最新后端进展：`agent-store` 已承接 channel 映射/幂等表与配置档案持久化，`apps/agent-server` 已补齐 `/api/channels` 控制面，并把飞书 channel 从过渡 webhook 入口收口为正式长连接 worker。当前长连接会按 profile 启停与重连，收到事件后先快速确认，再异步走既有 `session_manager.submit_turn(...)` 主链与飞书回复链路。
- 最新 channel 持久化收口：`channel-registry` crate 已从工作区移除；`ChannelProfile` 现统一由 `channel-bridge::ChannelProfileRegistry` 作为 store façade 管理，`apps/agent-server` 启动时直接从 `agent-store` 的 `channel_profiles` 表加载。`/api/channels` 的增删改也已先写 SQLite 再更新内存快照，避免 store 失败后 runtime 继续拿到脏 profile 状态。
- 最新 channel 抽象进展：`channel-bridge` 现在不只承接 session 绑定恢复、turn 前预压缩与消息回执幂等 helper，还新增了通用 `ChannelRuntimeAdapter` / `ChannelRuntimeSupervisor` 与去平台化的 `ChannelRuntimeHost` / `ChannelRuntimeEvent` 边界；与此同时新增 `channel-feishu` crate，飞书 WebSocket、CardKit、回复控制与协议实现已从 `apps/agent-server` 迁出，`agent-server` 现在只保留宿主注册、状态装配与 SSE→通用 channel 运行时事件映射。
- 最新飞书修复进展：私聊回复路径已从“总是 reply 当前消息”收紧为“私聊按 `chat_id` 直发、群聊按需 reply/thread”；SSE 现已补齐 `current_turn_started` 事件，前端对外部 channel 触发的当前执行态支持即时显示与流式恢复，不再依赖手动刷新；飞书消息进入处理中后会先加 `Typing` 表情，结束后再移除；同时飞书回包已进一步切到 `CardKit`：先创建 card entity，再用 `element_id` 按流事件更新正文，最后关闭 streaming mode 并收口为最终卡片。
- 最新飞书消息覆盖修复：同一个 turn 内若模型产出多段 assistant message，飞书最终卡片不再只取最后一段 `assistant_message`，而是优先按 `TurnBlock::Assistant` 顺序拼接所有回复块，避免后一段覆盖前一段。
- 最新飞书流式关联修复：飞书 `CardKit` 回复链已从“按 `session_id` 监听 SSE”升级为“按 server 侧 `turn_id` 监听 SSE”；`CurrentTurnStarted/Status/Stream/Error/TurnCancelled/TurnCompleted` 现都携带同一轮的 turn 关联 ID，跨 turn 不再共享同一张流式卡片。
- 最新飞书控制器对齐进展：回复管线已进一步收口成 per-dispatch controller 形态；单个 turn 现在由一个独立 controller 持有 `reply_mode + 卡片状态 + flush 时钟`，流中文本与完成态段落分离累计，更接近 `openclaw-lark` 的单 controller / 单卡片模型。
- 最新飞书后台删除修复：session 在后台被删除后，`channel_session_bindings` 现在会一并清理；即使历史脏数据残留，飞书入口在发现 binding 指向已删除 session 时也会自动新建 session 并回写绑定，不再出现“只记日志、不回消息”的 stale binding 黑洞。
- 最新 turn 时序修复：`StreamEvent::Done` 到达后，server 现在会立刻把当前 turn 状态切到 `finishing`，前端可立即从“仍在生成”切到“收尾中”；同时 tool trace 持久化已移出 `turn_completed` 之前的关键路径，避免工具 trace 写盘继续阻塞终态 SSE。
- 当前步骤：在 Web + server 主路径稳定的基础上，继续收口“可作为其他客户端驱动接口”的 server 形态，并把全异步主链推进到 Phase 4 的原生 async 收口态：`builtin-tools` 的文件/搜索工具、`agent-core` / `agent-runtime` 的 async + `Send` 边界，以及 `apps/agent-server` 的 session manager / turn 执行都已切到 Tokio async task；当前又完成了一轮事件桥接可靠性收口：`/api/events` 在 `tokio::broadcast` 消费者落后时不再静默吞掉 `Lagged` 错误，而会显式发出 `sync_required` 事件；`apps/web` 收到该事件后会主动重拉 session 列表与当前 active session 的 `history/current-turn/info`，把 SSE 在线分发层与 session tape / snapshot 的持久化恢复边界真正接上。与此同时，`LanguageModel` 已收口为单一 `complete_streaming(request, abort, sink)` 入口，`agent-runtime` 对外 turn 入口也已收口为单一异步 `handle_turn_streaming(user_input, control, sink)`，上下文压缩入口也已只保留异步 `auto_compress_now()`，并且压缩请求现在会生成独立 trace context、落到本地 trace store；Web 侧又进一步把普通对话 trace 与 compression 日志拆成独立视图，避免压缩调用继续混进现有 trace 列表；与此同时，trace 首屏数据也已改成单次 `/api/traces/overview` 读取，前端同页重复刷新会被 store 合并，`agent-store` 还为 `span_kind/request_kind/trace_id/started_at_ms`、`trace_id` 与 `duration_ms` 热路径补上复合索引，减少单连接 SQLite 下的全表扫描与串行放大；`runtime::tool_calls` 里的 runtime-tool / 普通 tool 生命周期记账逻辑不仅已收口到共享 helper，也已进一步按 `tool_calls::{execute,lifecycle,types}` 模块化；`agent-core::CompletionRequest`、`agent-runtime` 与 `openai-adapter` 现已补齐 `parallel_tool_calls` 共享开关，Responses / Chat Completions 请求会显式发送该字段，而 runtime 也开始按“只读类工具可并行、写入/交互类工具串行”执行同一批工具调用；`agent-store` 的 SQLite 访问也已统一经由 `AiaStore::with_conn(...)` 显式包住锁边界，并新增了共享 `first_session_id()` / `SessionRecord::new(...)` helper，减少 `apps/agent-server` 在默认 session 解析和 session 记录构造上的重复样板；现在 trace 列表页又改为优先读取 `request_summary.user_message`，避免在列表接口里为每一行都反序列化完整 `provider_request` 大 JSON，同时 `apps/agent-server` 也开始按 `request_kind` 独立过滤普通 trace 与 compression 日志；`apps/agent-server` 的 live current-turn 更新与 tape→snapshot 重建也已开始共用 `runtime_worker::projection` helper，避免 `CurrentTurnBlock` / `CurrentToolOutput` 投影语义继续在 `session_manager` 与 `runtime_worker` 两边漂移；`openai-adapter` 里两条协议共享的 HTTP/request helper、协议专属 payload 类型，以及流式请求驱动 / SSE transcript 解析也已分别收口到共享模块与协议子模块；旧的 `complete` / `complete_streaming_with_abort` / `handle_turn*` / `block_on_sync(auto_compress_now_async)` 同步包装已移除，跨协议共用的 `payloads.rs` 也已拆散；下一批热点集中在 `openai-adapter` 剩余协议特有的 delta/tool-call 累积细节，以及 `agent-store` / `apps/agent-server` 之间还能继续下沉的共享查询/投影逻辑

## 已完成

- 建立 Rust 工作区
- 建立 `aia-config`
- 建立 `agent-core`
- 建立 `session-tape`
- 建立 `agent-runtime`
- 建立 `provider-registry`
- 建立 `openai-adapter`
- 建立 `agent-store`
- 建立 `apps/web` Web 工程骨架并演进为实际主工作台
- 完成最小可运行验证与基础测试覆盖
- 完成项目级命名从 `like` 收敛为 `aia`
- 完成本地 provider 注册、活动项持久化与协议类型区分
- 完成 OpenAI Responses 与 OpenAI 兼容 Chat Completions 双协议适配
- 完成会话磁带扁平化、锚点、handoff、fork / merge、查询切片与旧格式兼容
- 完成结构化工具调用 / 工具结果贯穿运行时主链，并统一沉淀为可重建请求的会话条目
- 完成 OpenAI Responses 与 Chat Completions 的原生工具链路映射
- 完成运行时单轮多步模型 / 工具循环、重复工具调用防重、预算提示、文本收尾步与独立工具调用上限
- 完成 `apps/web` 首页从模板页替换为项目主界面骨架
- 完成 `apps/web` 工作台首页重构，并接入基础 UI 组件体系
- 完成 Web 主界面信息结构收敛：左侧边栏、中央消息列表、底部输入框
- 完成 `docs/frontend-web-guidelines.md`，明确 Web 前端开发规范与运行时边界
- 完成 `apps/agent-server` axum HTTP+SSE 服务器，桥接 Web 前端到共享运行时
- 完成全局 SSE 事件流架构（`GET /api/events`），基于 `broadcast::channel` 向所有客户端推送事件
- 完成 `POST /api/turn` fire-and-forget 消息提交，响应通过全局 SSE 返回
- 完成 provider、session、history、current-turn、handoff、cancel、trace 等主接口
- 完成 Rust 侧核心类型（`StreamEvent`、`TurnLifecycle`、`TurnBlock` 等）的序列化支持
- 完成前端全局 store：统一管理 SSE 连接、流式状态累积、turn 完成回收与 provider 当前状态刷新
- 完成流式轮次状态指示：waiting → thinking → working → generating
- 完成流式 tool output 实时渲染，按 invocation_id 分组展示，不等 turn 完成
- 完成 Vite 开发代理配置（`/api` → `http://localhost:3434`）
- 完成移除 `apps/agent-cli` 包，并同步清理工作区与文档中的 CLI 主入口叙事
- 完成核心 Rust crates 的内部模块化收口：`provider-registry`、`agent-core`、`session-tape`、`openai-adapter`、`agent-runtime`、`aia-config` 已保持薄 façade + 职责模块
- 完成 `provider-registry` 与 `apps/agent-server` 之间的 provider 多模型配置接口对齐，并兼容旧单模型本地落盘格式
- 完成 `agent-runtime` 深一层内部拆分：主循环、请求构造、工具执行、事件缓冲、错误与测试已进一步解耦
- 完成内建编码工具命名收口为 `shell`、`read`、`write`、`edit`、`glob`、`grep`
- 完成 `shell` 内建工具改为内嵌 `brush` 库执行，并补齐 stdout / stderr / exit_code 结构化回传与基础测试
- 完成 `apps/agent-server` 向 runtime 显式传入 `workspace_root`，保证相对路径工具调用语义稳定
- 完成 Web 端 provider 创建、更新、删除、切换与当前 provider / provider 列表刷新链路
- 完成 Web 端 Channels 配置链路：`AppView`/Sidebar/MainContent 已接入 `channels` 视图，前端 store 与 `/api/channels` 的 list/create/update/delete 已连通，当前先只支持飞书 channel
- 完成 channel profile 持久化迁移：已配置 channel 档案当前统一落盘到 `.aia/store.sqlite3`
- 完成 `channel-registry` crate 退场：共享 profile 模型与 store façade 已并入 `channel-bridge`，旧 `.aia/channels.json` 路径约定也已从共享配置中移除
- 完成 channel 配置模型再收口：`ChannelProfile` 现只保留通用 profile 元数据 + raw `config` payload，具体配置结构与校验改由 adapter 自己定义；旧版 Feishu 裸配置 JSON 继续可读
- 完成 channel catalog 首轮打通：`channel-bridge` 现在额外提供 `ChannelAdapterCatalog` 与 `SupportedChannelDefinition.config_schema`；`agent-server` 已暴露支持的 channel catalog，Web 端 Channels 面板可按 server 下发的 schema 动态渲染，而不是继续写死 Feishu 表单
- 完成命名与边界收口：代码层把“已配置渠道档案”的 store façade 统一命名为 `ChannelProfileRegistry`，并把运行时支持的实现目录明确命名为 `ChannelAdapterCatalog`，避免两类 registry 继续混淆
- 完成 `channel-bridge` 首轮 trait 化落地：共享 `ChannelSessionService` / `ChannelBindingStore` 契约、session 绑定恢复、turn 预压缩、消息回执幂等 helper，以及 adapter 化 `ChannelRuntimeSupervisor` 已从 `apps/agent-server` 抽离，供多渠道入口复用
- 完成 `channel-feishu` 首轮迁移：飞书协议、长连接、回复控制、卡片流式状态与相关单测已迁入独立 crate，`agent-server` 只保留宿主桥接
- 完成 `agent-store` 的 channel 动态索引：外部会话键 → `session_id` 映射与 `message_id` 幂等去重已进入 SQLite
- 完成飞书 channel 正式长连接桥接：`apps/agent-server` 当前会按 channel 配置维护飞书长连接 worker，复用既有消息解析、会话映射、SQLite 幂等去重、`session_manager.submit_turn(...)` 与回复发送链路；原 webhook 过渡入口已移除
- 完成飞书入口确认路径收口：长连接事件收到后会先回确认帧，再异步执行自动压缩、turn 提交、等待 SSE 完成与飞书回复，避免把平台 3 秒确认窗口直接绑死在模型/工具执行时长上
- 修复飞书私聊 `open_id cross app`：私聊场景不再走 `sender.open_id` 直发，而是优先使用 p2p `chat_id`（从事件载荷或 `chat_p2p/batch_query` 解析）作为 `receive_id_type=chat_id` 的发送目标；群聊仍保留 reply / thread 语义
- 修复飞书外部消息刷新后才可见：server 发送 `current_turn_started`，web 在缺少本地 `streamingTurn` 时会主动从 `/api/session/current-turn` 恢复当前执行态，支持外部消息即时渲染与流式恢复
- 修复飞书配置热更新的失活 worker 漏重启：同 fingerprint 但已退出的长连接 worker 现在也会在 `sync_feishu_runtime(...)` 中被重新拉起，不再卡到只能靠进程重启恢复
- 完成飞书处理中表情生命周期：收到用户消息开始处理时会对原消息添加 `Typing` reaction，处理结束后按返回的 `reaction_id` 删除，避免“处理中”状态残留
- 完成飞书 CardKit 流式回复：收到消息后会先创建 `CardKit` card entity，再发送引用 `card_id` 的 interactive 消息；处理中正文通过单个 markdown `element_id` 流式更新，最终关闭 streaming mode 并更新为完整卡片
- 收紧飞书卡片视觉层级：按“直接回复内容”的格式呈现，不再额外显示 header 和用户消息区块；完成态保留可折叠思考过程、工具状态和耗时 footer
- 修复同一 turn 多段回复覆盖：完成态回复内容现改为从 `turn.blocks` 中提取全部 assistant 块并按顺序拼接；如果没有 assistant 块才回退到单段 `assistant_message`
- 收口跨 turn 串写风险：aia 当前飞书桥接已补上 server 侧 `turn_id` 关联，`submit_turn` 会返回本次 turn 关联 ID，飞书 bridge 和 self-chat 都按 `session_id + turn_id` 双键过滤流式事件；`openclaw-lark` 的 per-dispatch controller 隔离思路已在这一层被对齐到可用语义
- 对齐 `openclaw-lark` 的累计语义：流中文本现在独立保存为 `streaming_text`，完成态段落保存为 `completed_segments`；最终卡片优先展示完成段落，流中卡片优先展示正在累积的正文，这更接近 `openclaw-lark` 的 `accumulatedText/completedText` 分离模型
- 修复后台删除后的飞书会话失联：删除 session 时会同步删除该 session 对应的 channel binding；对于旧版本遗留的脏 binding，飞书入口也会在 `resolve_session_id(...)` 阶段做活性检查并自动 rebinding 到新 session
- 缩短 turn 终态感知延迟：前端原先只在 `turn_completed` 才结束活跃状态；当前已新增 `finishing` 终末状态，并在 `StreamEvent::Done` 后立即广播，使 UI 不再长时间停留在 `generating`。同时 `persist_tool_trace_spans(...)` 改为异步后台写入，减少完成事件前的尾延迟
- 完成 provider 变更的事务式提交：候选 registry 校验、registry 落盘、session tape 落盘全部成功后才提交到内存 runtime / tape
- 完成 provider 持久化失败路径回归测试，保证落盘失败不会留下 registry / runtime / tape 分叉状态
- 完成 Web 端 Markdown 渲染入口收敛为共享前端组件，并统一消息排版样式
- 完成 `apps/agent-server` 运行时拥有关系重构：由后台 runtime worker 独占 `AgentRuntime`、provider registry 与 session 持久化，HTTP 路由通过消息传递访问运行时
- 完成 provider 当前信息 / provider 列表快照化读取，长时间 shell / model turn 不再阻塞轻量查询接口
- 完成 session history / current-turn 快照化读取：运行中的 agent loop 不再把 `/api/session/history` 挂起，页面刷新时也能直接恢复当前进行中的 turn
- 完成 session jsonl 实时 append 落盘：agent loop 过程中新增的用户消息、thinking、tool 调用结果与完成/失败事件都会立即写入 `.aia/session.jsonl`
- 完成 Web 端用户消息的乐观渲染，提交后立即显示到消息列表，而不是等流式完成再落入 completed turn
- 完成 trace 记录对真实 HTTP 状态码的保留：不再在成功路径硬编码 `200`
- 完成 trace 列表按 agent loop 聚合展示与 recent loops 分页
- 完成共享协议层与 trace/UI 对 provider 私有 checkpoint 概念的移除
- 完成 Web trace 页收口为更接近 tracing 产品的三栏视图：左侧 recent loops，中间 span timeline，右侧 inspector
- 完成 `agent-store` 侧本地 span store 能力：LLM spans 与 tool spans 共用本地 trace/root span 语义
- 完成 `llm-trace` 本地 event timeline 落盘：记录 request started、首个 reasoning/text delta、tool-call detected、response completed/failed
- 完成 runtime tool span 的后端真实落盘：工具执行不再只是前端临时推导节点
- 完成流式工具事件语义拆分：`tool_call_detected` 与 `tool_call_started` 不再混用
- 完成 `tape_info` / `tape_handoff` 从 runtime 特判式实现收口到 `Tool` trait + runtime tool registry
- 完成真实 token usage 贯通到 turn 主链、session history 与 Web 展示
- 完成自动上下文压缩触发修正与 `context_compressed` 可观测性补齐
- 完成提交前的后端自动压缩收口：高压力下会先 idle auto-compress 再启动 turn
- 完成 Web 历史消息体验优化：切换 session / 水合历史时直接跳到底部，历史按页加载
- 完成 Web 历史分页交互收口：消息列表上滚接近顶部时自动加载前一页，替代显式点击按钮
- 完成 Web 历史翻页定位重构：只以 `user message` 作为上下文锚点恢复位置，并关闭浏览器默认滚动锚定，减少提前触发加载时的顶跳与漂移
- 完成 Web 历史翻页锚点补偿修正：加载前一页后优先按首个可见消息锚点恢复视口，减少定位漂移
- 完成 Web session 切换流畅度收口：store 维护按 session 的本地快照缓存，切换时保留上一帧内容并显示轻量 loading 提示，不再先清空消息区造成闪烁
- 完成 Web session 切换滚动收口：每次切换 session 时都强制跳到最新消息底部，不再保留旧会话的局部滚动位置
- 完成 Web 聊天列表首轮渲染减载：消息项引入 memo，长历史列表改为轻量窗口化渲染，并按 session 恢复滚动位置；历史分页加载时不再意外强制滚到底部
- 完成 Web 聊天列表第二轮滚动/窗口化收口：窗口化从估算高度升级为动态测量高度，切换 session 时明确滚动到最新消息底部，避免旧会话中段位置残留带来困惑
- 完成 Web 聊天列表第三轮锚定稳定性收口：已移除动态测量窗口化与锚定补偿机制，优先选择更稳定、可预测的消息渲染路径，避免流式与展开/收起场景下的潜在抖动
- 完成 Web session 切换首屏收口：切换前仅同步保存旧 session 的最后一个 turn 快照；切入新 session 时先展示/拉取最后一个 turn，再后台补齐其余历史，减少主线程阻塞与首屏等待
- 完成 Web session 快照瘦身：`_sessionSnapshots` 退化为最小 UI snapshot，只保留最后一个 turn 与 streaming/UI 状态，不再长期缓存历史页副本
- 完成 Web session 后台补历史收口：首屏只进最后一个 turn，其余历史改为空闲时增量补页，并在切走 session 时取消后台补页，减少与滚动/streaming 的竞争
- 完成 Web idle 调度抽象：session 后台补历史不再写死 `setTimeout`，优先使用浏览器 `requestIdleCallback`，并保留 fallback 与测试可控注入
- 完成 Web 端 turn 提交请求的 `keepalive` 加固
- 完成 provider 注册表加载的旧路径兼容：当 `.aia/providers.json` 缺失时，自动回退读取 `.aia/sessions/providers.json`
- 完成完整的 stop/cancel 基线：server 暴露 `POST /api/turn/cancel`，session manager 能中断运行中 turn，runtime 把取消信号传到工具执行上下文，Web 输入区提供 stop 按钮并显示 cancelled 状态
- 完成 stop/cancel 第二阶段基线：runtime 会把 abort 继续传到 OpenAI streaming 调用；embedded `brush` shell 在收到取消后会向当前作业发送 `TERM` 并尽快收尾；`TurnLifecycle` 新增共享 `outcome` 字段；server 取消 API 只负责触发 abort，真正的 cancelled SSE 由 worker 在轮次结束时统一发出一次
- 完成全异步主链 Phase 1 收口：`agent-core` 的 `LanguageModel` / `ToolExecutor` / `Tool` 已切换为 async trait，`agent-runtime` 新增 async turn 主链并保留同步包装入口，相关 mock / 测试实现也已统一迁到 async trait 用法
- 完成全异步主链 Phase 2：`openai-adapter` 已从 `reqwest::blocking` 切到 async `reqwest`，Responses / Chat Completions 的单次请求与流式 SSE 都改为原生 async HTTP / chunk streaming，同时保留 abort / cancel 语义
- 完成全异步主链 Phase 3 的关键长任务路径收口：`builtin-tools` 的 `shell` 已把 stdout/stderr 聚合、abort 轮询与输出捕获都改为 async 事件泵，并移除自建 thread + current-thread runtime；当前 `brush` 执行直接挂在 Tokio task 上，输出改为异步 tail 临时 capture 文件，不再依赖 `spawn_blocking`
- 完成全异步主链 Phase 3 的内建文件/搜索工具收口：`read` / `write` / `edit` 已切到 `tokio::fs`，`glob` / `grep` 也已改为共享的 async `.gitignore` 感知仓库遍历 + async 文件读取，不再依赖 `spawn_blocking` / `ignore::WalkBuilder` 扫描仓库
- 完成全异步主链 Phase 4 的 server 原生 async 收口：`apps/agent-server` 的 session manager 与 turn 执行都已切到 Tokio async task，移除了 `tokio::spawn_blocking`、`std::thread::Builder`、`LocalSet` 与 `spawn_local`；运行中 `session/info` 也改为读取内存中的 `ContextStats` 快照，而不是回退磁带
- 完成 trace 查询路由的 async 控制面收口：`/api/traces`、`/api/traces/{id}` 与 `/api/traces/summary` 已去掉 per-request `spawn_blocking` 包装，直接复用共享 SQLite store 读取路径，并补齐了路由回归测试
- 完成 `apps/agent-server` 路由模块化：`routes.rs` 不再承载全部 provider/session/trace/turn handler，现已拆分为 `provider`、`session`、`trace`、`turn`、`common` 与独立测试模块，并把重复的 session 解析、JSON/error/ok 响应 helper 收口到共享模块
- 完成 `apps/agent-server` 的 `session_manager` 模块化：主文件只保留 session loop、slot 生命周期与 provider/runtime 同步逻辑；命令发送模板、共享类型、current-turn 流式投影、tool trace 持久化与测试都已分别拆到 `session_manager::{handle,types,current_turn,tool_trace,tests}`
- 完成 `apps/agent-server` 的 `model` 模块化：主文件只保留 `ServerModel`、provider 选择与 trace 落盘主流程；bootstrap mock、trace helper 与测试分别拆到 `model::{bootstrap,trace,tests}`
- 完成 `agent-store::trace` 模块化：根文件只保留 trace 类型与 trait；schema 初始化、store 实现、row 映射/JSON 解码与测试分别拆到 `trace::{schema,store,mapping,tests}`，并收口了重复的 JSON 列解析逻辑
- 完成 `apps/agent-server` 的 `runtime_worker` 模块化：根文件只保留薄 façade；共享类型、tape 快照重建/legacy decode helper 与测试分别拆到 `runtime_worker::{types,snapshots,tests}`
- 完成 `agent-runtime::runtime::turn` 模块化：根文件只保留薄入口；turn 主驱动、completion segment 处理与共享 turn buffer / success-failure context 已分别拆到 `turn::{driver,segments,types}`，并收口了重复的失败上下文拼装
- 完成 `builtin-tools::shell` 模块化：根文件只保留 `ShellTool` 契约与结果组装；capture 文件/事件泵、embedded brush 执行主流程与测试分别拆到 `shell::{capture,execution,tests}`
- 完成 `openai-adapter::responses` 模块化：根模块只保留 Responses 配置与模型入口；请求构造/HTTP helper、响应体解析、流式状态累积与 `LanguageModel` 客户端入口分别拆到 `responses::{request,parsing,streaming,client}`
- 完成 `openai-adapter::chat_completions` 模块化：根模块只保留 Chat Completions 配置与模型入口；请求构造/HTTP helper、响应体解析、流式状态累积与 `LanguageModel` 客户端入口分别拆到 `chat_completions::{request,parsing,streaming,client}`
- 完成 `openai-adapter` payload 类型按协议归位：原先跨协议共用的 `payloads.rs` 已拆成 `responses::payloads` 与 `chat_completions::payloads`，避免 Responses / Chat Completions 继续共存于同一组边缘层数据结构里
- 完成 `openai-adapter` 共享流式驱动收口：Responses / Chat Completions 的 `complete_streaming` 已共用顶层 streaming request driver、SSE transcript 记录与 `data:` JSON 行解析 helper，不再在两条协议里重复维护请求发送、状态码失败处理和 `[DONE]`/JSON 解析模板
- 完成 `LanguageModel` 历史入口清理：共享 trait 已收口为单一 `complete_streaming(request, abort, sink)`，runtime 压缩、server trace 桥接、OpenAI 适配器与相关测试/mocks 都已改走统一流式入口
- 完成 `agent-runtime::runtime::turn::driver` 失败路径收口：重复的 `record_turn_failure + return Err(...)` 样板已压缩为共享 `fail_turn` helper
- 完成 `agent-runtime` turn 公开入口清理：同步 `handle_turn` 与历史命名 `handle_turn_streaming_with_control_async` 已移除，对外统一为异步 `handle_turn_streaming(user_input, control, sink)`；`apps/agent-server` 与 runtime 测试已改走同一条异步入口
- 完成 `agent-runtime` 压缩入口清理：同步 `auto_compress_now` 包装与 `block_on_sync` helper 已移除，对外统一为异步 `auto_compress_now()`；`apps/agent-server` 的 session manager 已直接 await 共享 runtime 压缩入口
- 完成 `agent-runtime::runtime::tool_calls` 生命周期记账收口：runtime tool 与普通 tool 的成功/失败记录、事件发布与 `seen_tool_calls` 更新已改走共享 helper，减少重复分支和后续语义漂移风险
- 完成 `agent-runtime::runtime::tool_calls` 模块化：根文件只保留薄入口；工具调用主流程、生命周期落盘/事件发布与共享上下文类型已分别拆到 `tool_calls::{execute,lifecycle,types}`，并用共享构造 helper 收口重复的 lifecycle context / started event 样板
- 完成 `agent-store` SQLite 锁边界收口：session / trace / schema 初始化与 legacy 迁移已统一通过 `AiaStore::with_conn(...)` 访问共享 `Mutex<Connection>`，不再让各模块直接持有 `MutexGuard`；`session::update_session()` 也已去掉动态 SQL + `Box<dyn ToSql>`，改为显式分支
- 完成 `agent-store` / `agent-server` 的一轮 session helper 收口：`AiaStore::first_session_id()` 与 `SessionRecord::new(...)` 已下沉到共享 store/types 层，server 启动和路由默认 session 解析不再为“取第一条 session”整表加载，也不再在多个壳层重复手拼 `SessionRecord`
- 完成 trace overview 的 loop 级存储收口：`agent-store` 在 span 入库时同步维护 `llm_trace_loops` 聚合表，`/api/traces/overview` 现在直接按 agent loop 返回分页项，不再把单次模型调用误当作最终列表语义，也不再依赖前端临时按 `trace_id` 二次拼装
- 完成 `apps/agent-server` current-turn 投影 helper 收口：live stream 更新与 tape 快照重建共享 `runtime_worker::projection` 中的 `CurrentTurnBlock` / `CurrentToolOutput` 投影 helper，不再在 `session_manager::current_turn` 与 `runtime_worker::snapshots` 各自维护一套对象归一化、tool block 构造和状态推断逻辑
- 完成 `agent-runtime` / `openai-adapter` 的并行工具调用首轮落地：共享 `CompletionRequest` 新增 `parallel_tool_calls`，Responses / Chat Completions 请求会显式发送该字段；runtime 对同一批工具调用开始按策略执行——`read` / `glob` / `grep` 等只读类工具可并行准备与执行，而 `shell` / `write` / `edit` / runtime tools 继续保持串行，避免文件系统冲突与交互副作用
- 完成独立内建 `apply_patch` 工具：`edit` 保持单文件精确唯一替换语义不变，同时新增短名稳定的 `apply_patch` 工具承接 `*** Begin Patch` / `*** End Patch` 风格多文件补丁，支持 `Update File`、`Add File`、`Delete File`，让 Codex/Claude 风格补丁映射无需借道 shell，也避免把两种编辑语义继续混在同一个工具里；其每文件结果元数据现也已收口为共享强类型结构，而不是继续在实现里手写 `serde_json::Value`
- 完成 SSE 落后客户端显式重同步：`apps/agent-server` 的 `/api/events` 在 `broadcast` 消费者落后时会发出 `sync_required` 事件，而不是静默丢弃；`apps/web` 收到后会主动补拉 session 列表与当前 session 的历史、当前 turn、上下文压力，避免事件流与本地 UI 状态无声漂移
- 完成工具参数 schema 共享 helper 收口：`agent-core::ToolDefinition` 除支持手写 JSON 外，也支持基于自研最小 `ToolArgsSchema` trait 与 derive 宏生成统一参数 schema；当前 `builtin-tools`、runtime tape tools 与测试中的常规参数结构体已切到 `#[derive(ToolArgsSchema)]` 自动生成，`ApplyPatchToolArgs` 也已收口为单 struct + 别名字段模型来复用这条能力；该 helper 继续只覆盖当前真实需要的 object/properties/required/additionalProperties/description/minimum/minProperties 子集，避免再引入外部反射式 schema 依赖
- 补齐 `ToolArgsSchema` 用户态清单文档：新增 `docs/tool-schema-derive.md`，明确支持的字段类型、结构边界、`tool_schema(...)` 可用键与 `serde` 协作范围，避免把可发现性完全寄托在编辑器对 derive helper attribute 的内部键提示上
- 扩展 `ToolArgsSchema` derive 高频能力：当前已补 `bool` / `Option<bool>`、有符号整数族、`Vec<String>` / `Option<Vec<String>>`，以及字段级 `minimum` / `maximum` 数值约束，让下一批简单工具参数不必再回退到手写 schema
- 完成 `ToolArgsSchema` compile-fail 诊断回归：`agent-core` 现用 `trybuild` 锁住容器级/字段级非法 `tool_schema(...)` 键与无符号负数约束等关键错误文案，避免 derive 宏后续扩展时悄悄把用户态诊断弄差
- 完成真实工具调用到 typed args 的统一收口：`agent-core::ToolCall` 新增共享 `parse_arguments()`，当前 `builtin-tools` 与 runtime tools 的 `call()` 已改为直接反序列化结构化参数，而不再手工散落 `str_arg/opt_*_arg/arguments.get(...)` 取值
- 完成真实工具 description 的集中管理：`agent-prompts` 现通过 `prompts/tool/` 目录下的 Markdown 文件统一管理内建工具与 runtime tools 的共享 description，`builtin-tools` / `agent-runtime` 的真实 `ToolDefinition` 已改为复用这些文本，不再各 crate 自带字面量
- 完成 `agent-store` async façade 收口：session / trace 的 SQLite 访问已通过共享 async store API 暴露给 `apps/agent-server` 与 `ServerModel`，trace/session 路由、session manager 初始化与 turn/tool trace 落盘不再在 async 路径里直接调用同步 store 方法
- 完成 `agent-store` SQLite 锁中毒恢复：trace/session 读写与 schema 初始化不再因 `Mutex<Connection>` poisoned 而 panic
- 完成 `aia-config` 共享配置 crate：把 `.aia` 路径、默认 session 标题、server 默认地址 / 事件缓冲 / 请求超时、统一 user agent 组装，以及 trace / span / prompt-cache 稳定前缀从 `apps/agent-server` 与相关共享 crate 中收口
- 完成 `aia-config` 内部模块化：拆为 `paths`、`server`、`identifiers` 三类共享配置模块，`lib.rs` 保持薄 façade
- 完成 `provider-registry`、`session-tape`、`apps/agent-server`、`agent-runtime` 对共享配置默认值与 helper 的首轮接入
- 完成 `apps/agent-server` 启动路径错误收口：provider 注册表、SQLite store、sessions 目录、默认 session、模型构建、端口绑定与 server serve 失败不再 `expect` panic
- 完成 `runtime_worker` 历史重建解码告警收口：legacy `turn_record` 与 `turn_completed.usage` 损坏时不再无声忽略，而会输出明确诊断并尽量保留其余可重建轮次数据
- 完成 `agent-core` / `agent-runtime` 时间辅助函数收口：tool invocation id、turn id 与时间戳生成在系统时钟回拨到 `UNIX_EPOCH` 之前时不再 panic
- 完成 `builtin-tools` shell 测试稳定性修正：stdout delta 断言不再假设嵌入式 shell 只会回传单个输出块
- 完成 `apps/web` 工具链切换到 Vite+ 工作流，并引入子目录级 `apps/web/AGENTS.md` 约束
- 完成 trace 列表读取瘦身：`agent-store` 的列表查询改为从 `request_summary.user_message` 读取轻量用户消息预览，不再为每条列表项反序列化完整 `provider_request`
- 完成上下文压缩调用 trace 化：`agent-runtime::auto_compress_now()` 现在会生成独立压缩 trace context，`apps/agent-server` 会把压缩请求持久化到 trace store，`apps/web` 的 trace 面板也会显式标识 compression activity
- 完成工具参数 schema 对外收口：当前 `builtin-tools` 与 runtime tape tools 已统一通过手写裸 JSON 或 derive schema helper 暴露稳定外部契约；`apply_patch` 参数也已从未标记枚举收口为单 struct + `patch` / `patchText` 可选别名字段模型，在保持原有兼容语义的同时复用 derive 能力；共享 schema 归一化仍保留为后备能力，用于清洗手写 JSON 与 derive helper 的输出细节
- 完成 compression 日志独立视图：`apps/agent-server` 的 trace 列表/汇总已支持按 `request_kind` 过滤，`apps/web` 把普通对话 trace 与上下文压缩日志拆成独立视图，不再混合展示
- 完成 trace 首屏请求合并与查询提速：`apps/agent-server` 新增 `/api/traces/overview` 单次概览读取，`apps/web` 的 trace store 会合并同页重复刷新，`agent-store` 也已为 trace 列表/汇总热点查询补齐复合索引
- 修正 `/api/traces/overview` 分页语义：`agent-store` 的 trace page 现真正按返回 `items` 数量分页，`page_size` 不再只是“loop 数上限”；同时 overview summary 也已落到本地 SQLite 快照表 `llm_trace_overview_summaries`，减少每次请求重新聚合扫描
- 完成 `agent-server` 基础 CLI 双入口：二进制默认仍启动 HTTP+SSE server，同时新增 `self` 子命令读取 `docs/self.md` 并直接进入终端对话；其 turn 提交、自动预压缩与事件消费复用同一套 session manager / runtime 主链，而不是另造一条 CLI 专用 agent loop
- 完成 `agent-server self` 首批内建命令：`/help`、`/status`、`/compress`、`/handoff <name> <summary>` 已接到现有 session manager 命令面，便于在终端自我进化模式下查看命令说明、上下文压力、手动压缩和创建 handoff，而不必回到 Web；格式错误的内建命令也会在本地直接返回 usage，而不是误发给模型
- 完成 Web 聊天区一次 session 切换滚动抖动收口：`ChatMessages` 在切换 session 时改为用 `useLayoutEffect` 同步恢复到底部，避免首帧先渲染旧滚动位置再跳动到最新消息
- 完成 `read` 工具元信息文案纠偏：聊天内工具时间线里，`read` 的 meta 改为显示真实 1-based 行号范围（如 `L121-160`），不再把 `offset` 和 `lines_read` 误显示成含糊区间

## 正在进行

- 收口 runtime worker 留在 `apps/agent-server`、哪些能力适合上移到 `agent-runtime` 的边界
- 继续评估 `channel-feishu` 与 `channel-bridge` 之间是否还有可再压缩的宿主接口；若未来出现第二个 transport，再只抽已证明重复的部分，避免过早泛化
- 观察内嵌 `brush` 作为 shell 运行时的实际稳定性、命令兼容性与中断语义
- 继续把 trace 数据模型从“本地 span store + event timeline”推进到更完整的 resources / richer events 模型，但暂不抢在工具协议映射与 MCP 之前做 exporter / collector 集成
- 继续观察 provider settings 与 channels settings 这两条配置面板在同一信息架构下的扩展性，避免后续新增更多配置面时把状态流和表单模式做散
- 继续补强飞书 channel 的 mention / 群权限策略、可用范围与白名单控制，避免长连接主链落地后权限边界仍然过粗
- 验证 stop/cancel 目前对长时间 shell / 外部 provider streaming 的实际覆盖率；当前已打通 server→runtime→tool context，并进一步补上 OpenAI streaming 读取中的取消检查与 shell 作业 `TERM` 中断，后续仍需继续观察 provider/运行时在不同上游和复杂 shell pipeline 下的真实中断覆盖率
- 当前 OpenAI adapter 已切到 async `reqwest` + async chunk streaming；后续观察重点转为不同上游是否仍在连接建立、TLS、代理缓冲或服务端长时间不刷新的窗口里残留取消迟滞
- 全异步主链已完成 Phase 1-4：`shell`、文件工具、搜索工具、session manager / turn worker、runtime 公共 turn / 压缩 API、trace/session store 访问与 provider I/O 都已统一到 async 调用面；当前后续重点转为 runtime ownership / return-path 简化、共享层继续抽象，以及剩余实现样板的继续压缩
- 继续盘点跨 crate 的超大文件与重复逻辑热点；在 `routes`、`session_manager`、`model`、`runtime_worker`、`agent-store::trace`、`agent-runtime::runtime::turn`、`agent-runtime::runtime::tool_calls`、`builtin-tools::shell`、`openai-adapter::responses` 与 `openai-adapter::chat_completions` 完成拆分后，`agent-store` / `agent-server` 之间也已先后收掉默认 session 查询、记录构造、async store 边界与 current-turn 投影 helper 样板；下一批优先候选集中在 `openai-adapter` 剩余协议特有 delta / tool-call 累积 helper，以及 server/runtime ownership 路径的进一步收口
- 持续校准哪些跨 crate 应用级常量应该进入 `aia-config`，哪些应继续留在协议层、运行时或算法层
- 继续观察工具协议对外映射里的个别特殊参数形态，优先维持模型可见 schema 简洁稳定，而不是让内部反序列化联合体直接外泄

## 下一步

Web 侧 `Channels` 配置已补齐，但当前优先级不变：下一步仍优先推进统一工具协议映射与 MCP 接入，而不是继续扩张更多客户端表层能力。

1. 在全异步主链完成后，继续推进实现简化与按领域拆分：优先沿 `channel-bridge` / `channel-feishu` 新边界继续收口 `apps/agent-server::channel_host` 的宿主接口，再压缩 `session_manager` 的 runtime ownership / return-path 复杂度，并继续评估 `openai-adapter` 剩余协议特有 delta / tool-call 累积 helper 与 store 访问层中适合继续下沉或拆分的辅助逻辑
2. 在 async 主链与共享工具边界进一步稳定后，优先推进统一工具协议映射与 MCP 接入，而不是继续堆厚客户端界面
3. runtime 驱动辅助从 `apps/agent-server` 继续抽到共享层
4. 在工具协议边界进一步收稳后，把本地 trace 从当前 span record + event timeline 继续推进到更完整的 resources / richer events 形态
5. 桌面壳接入

## 暂时不做

1. 抢先做完整 OTLP exporter / collector 集成
2. 在共享协议边界未稳定前继续大幅扩展新的 app 壳层

## 为什么当前先做 Web，而不是继续堆终端界面

因为共享运行时、会话模型和工具协议主链已经稳定，继续维护独立终端壳只会增加重复界面成本。当前更合理的方向是让 `apps/web` 直接承接主界面，再由桌面壳复用同一 Web 前端与 Rust 核心；而在主界面主路径已经收口后，下一优先级应回到统一工具规范的外部映射与 MCP 接入，而不是继续提前堆厚更多客户端表层能力。

当前 trace 观测性也遵循同样原则：先把共享语义边界收稳，再谈 exporter 和外部 tracing 平台对接；如果工具协议和运行时事件边界还没完全稳定，就过早绑定某个 tracing 后端，只会让后续协议演进成本更高。

## 阻塞

- `apps/web` 的 Channels CRUD 前端链路当前无新增硬阻塞
- 当前无硬阻塞；已知非阻断事项主要是前端生产包体积提示偏大，以及 `shell` 的中断能力与长任务取消语义仍可继续增强
