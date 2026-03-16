# 项目状态

## 当前阶段

- 阶段：核心工作区搭建之后的当前细分步骤：Web 界面 ↔ 运行时桥接收口
- 当前步骤：在 Web + server 主路径稳定的基础上，继续收口“可作为其他客户端驱动接口”的 server 形态，并把 trace 诊断链路从“LLM 请求日志 + 前端推导 tool 节点”推进到“后端真实 span 持久化 + 前端直接消费真实 span”视角；同时已开始把应用级共享路径、默认值、稳定标识统一收口到 `aia-config`

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
- 完成 Web session 切换流畅度收口：store 维护按 session 的本地快照缓存，切换时保留上一帧内容并显示轻量 loading 提示，不再先清空消息区造成闪烁
- 完成 Web 聊天列表首轮渲染减载：消息项引入 memo，长历史列表改为轻量窗口化渲染，并按 session 恢复滚动位置；历史分页加载时不再意外强制滚到底部
- 完成 Web 聊天列表第二轮滚动/窗口化收口：窗口化从估算高度升级为动态测量高度，切换 session 时明确滚动到最新消息底部，避免旧会话中段位置残留带来困惑
- 完成 Web 聊天列表第三轮锚定稳定性收口：动态测量窗口化在超长工具输出展开/收起时会锚定当前首个可见 turn，减少视口跳动
- 完成 Web session 切换首屏收口：切换前仅同步保存旧 session 的最后一个 turn 快照；切入新 session 时先展示/拉取最后一个 turn，再后台补齐其余历史，减少主线程阻塞与首屏等待
- 完成 Web session 快照瘦身：`_sessionSnapshots` 退化为最小 UI snapshot，只保留最后一个 turn 与 streaming/UI 状态，不再长期缓存历史页副本
- 完成 Web 端 turn 提交请求的 `keepalive` 加固
- 完成 provider 注册表加载的旧路径兼容：当 `.aia/providers.json` 缺失时，自动回退读取 `.aia/sessions/providers.json`
- 完成完整的 stop/cancel 基线：server 暴露 `POST /api/turn/cancel`，session manager 能中断运行中 turn，runtime 把取消信号传到工具执行上下文，Web 输入区提供 stop 按钮并显示 cancelled 状态
- 完成 stop/cancel 第二阶段基线：runtime 会把 abort 继续传到 OpenAI streaming 调用；embedded `brush` shell 在收到取消后会向当前作业发送 `TERM` 并尽快收尾；`TurnLifecycle` 新增共享 `outcome` 字段；server 取消 API 只负责触发 abort，真正的 cancelled SSE 由 worker 在轮次结束时统一发出一次
- 完成 OpenAI 流式读流中断收口：`openai-adapter` 不再只在 `BufRead::lines()` 的逐行循环间隙检查 abort，而是通过后台按行泵送 + 主线程轮询 abort 的方式让 Responses / Chat Completions 的阻塞读流也能及时响应取消
- 完成 `agent-store` SQLite 锁中毒恢复：trace/session 读写与 schema 初始化不再因 `Mutex<Connection>` poisoned 而 panic
- 完成 `aia-config` 共享配置 crate：把 `.aia` 路径、默认 session 标题、server 默认地址 / 事件缓冲 / 请求超时、统一 user agent 组装，以及 trace / span / prompt-cache 稳定前缀从 `apps/agent-server` 与相关共享 crate 中收口
- 完成 `aia-config` 内部模块化：拆为 `paths`、`server`、`identifiers` 三类共享配置模块，`lib.rs` 保持薄 façade
- 完成 `provider-registry`、`session-tape`、`apps/agent-server`、`agent-runtime` 对共享配置默认值与 helper 的首轮接入
- 完成 `apps/agent-server` 启动路径错误收口：provider 注册表、SQLite store、sessions 目录、默认 session、模型构建、端口绑定与 server serve 失败不再 `expect` panic
- 完成 `runtime_worker` 历史重建解码告警收口：legacy `turn_record` 与 `turn_completed.usage` 损坏时不再无声忽略，而会输出明确诊断并尽量保留其余可重建轮次数据
- 完成 `agent-core` / `agent-runtime` 时间辅助函数收口：tool invocation id、turn id 与时间戳生成在系统时钟回拨到 `UNIX_EPOCH` 之前时不再 panic
- 完成 `builtin-tools` shell 测试稳定性修正：stdout delta 断言不再假设嵌入式 shell 只会回传单个输出块
- 完成 `apps/web` 工具链切换到 Vite+ 工作流，并引入子目录级 `apps/web/AGENTS.md` 约束

## 正在进行

- 收口 runtime worker 留在 `apps/agent-server`、哪些能力适合上移到 `agent-runtime` 的边界
- 观察内嵌 `brush` 作为 shell 运行时的实际稳定性、命令兼容性与中断语义
- 继续把 trace 数据模型从“本地 span store + event timeline”推进到更完整的 resources / richer events 模型，但暂不抢在工具协议映射与 MCP 之前做 exporter / collector 集成
- 验证 stop/cancel 目前对长时间 shell / 外部 provider streaming 的实际覆盖率；当前已打通 server→runtime→tool context，并进一步补上 OpenAI streaming 读取中的取消检查与 shell 作业 `TERM` 中断，后续仍需继续观察 provider/运行时在不同上游和复杂 shell pipeline 下的真实中断覆盖率
- 当前 OpenAI adapter 已把 SSE 读流取消从“逐行检查”推进到“阻塞读期间也能轮询 abort”；后续观察重点转为不同上游是否仍在连接建立、TLS、代理缓冲或服务端长时间不刷新的窗口里残留取消迟滞
- 持续校准哪些跨 crate 应用级常量应该进入 `aia-config`，哪些应继续留在协议层、运行时或算法层

## 下一步

1. 继续观察并补强 stop/cancel 在不同 OpenAI 兼容上游与复杂 embedded shell pipeline 下的实际中断覆盖率，当前 OpenAI adapter 已能在阻塞读流期间及时响应 abort，下一步重点转向连接建立 / 代理缓冲 / 非 OpenAI provider 的剩余阻塞窗口
2. runtime 驱动辅助从 `apps/agent-server` 继续抽到共享层
3. 在工具协议边界进一步收稳后，把本地 trace 从当前 span record + event timeline 继续推进到更完整的 resources / richer events 形态
4. 继续补强 shell 中断 / 长任务处理与更细粒度的工具运行时能力
5. 桌面壳接入

## 暂时不做

1. 抢先做完整 OTLP exporter / collector 集成
2. 在共享协议边界未稳定前继续大幅扩展新的 app 壳层

## 为什么当前先做 Web，而不是继续堆终端界面

因为共享运行时、会话模型和工具协议主链已经稳定，继续维护独立终端壳只会增加重复界面成本。当前更合理的方向是让 `apps/web` 直接承接主界面，再由桌面壳复用同一 Web 前端与 Rust 核心；而在主界面主路径已经收口后，下一优先级应回到统一工具规范的外部映射与 MCP 接入，而不是继续提前堆厚更多客户端表层能力。

当前 trace 观测性也遵循同样原则：先把共享语义边界收稳，再谈 exporter 和外部 tracing 平台对接；如果工具协议和运行时事件边界还没完全稳定，就过早绑定某个 tracing 后端，只会让后续协议演进成本更高。

## 阻塞

- 当前无硬阻塞；已知非阻断事项主要是前端生产包体积提示偏大，以及 `shell` 的中断能力与长任务取消语义仍可继续增强
