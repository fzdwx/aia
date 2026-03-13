# aia：第一阶段架构

## 目标

这份骨架先解决三件以后最难改的事：

1. **统一代理核心**：终端界面和桌面壳共享同一套运行时，不重复造轮子。
2. **可追加的会话磁带**：把上下文、压缩、交接、分叉都建立在扁平的 `{id, kind, payload, meta, date}` 事件流之上，对齐 republic 数据模型。
3. **可兼容的工具协议**：内部只维护一套工具定义，向外可映射到不同模型或协议。

## 为什么先做库，不先做界面

README 里真正难的是这些能力：

- 模型人格差异感知
- 工具系统、子代理、异步子代理
- 增量压缩与交接
- 兼容不同外部工具规范

这些如果先揉在界面里，后面会很难拆。第一阶段因此采用“库优先”：

- `agent-core`：领域模型与协议边界
- `session-tape`：扁平条目磁带（`{id, kind, payload, meta, date}`）、轻量锚点、handoff 事件、查询切片、fork / merge 与重建状态
- `agent-runtime`：运行时编排与最小 turn 执行
- `provider-registry`：provider 资料、活动项与本地持久化
- `openai-adapter`：首个真实模型适配层，负责把统一请求映射到 Responses 风格接口
- `agent-cli`：最小可运行入口，用来验证核心设计；输出二进制名为 `aia`

## 模块边界

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
- 既保留面向通用展示的扁平消息投影，也保留面向模型续调的结构化会话投影
- 额外保留模型检查点事件，用于像 OpenAI Responses 这类需要 `previous_response_id` 的续调协议
- 通过 jsonl 快照文件落盘可重放条目流，新格式直接序列化扁平 entry
- 追加式文件存储与内存存储都围绕”每个磁带一条独立日志”建模
- 分叉 / 合并只追加增量，不重写主线条目
- 旧格式兼容：载入时识别 `{id, fact, date}` 旧 JSONL 并自动转换为扁平条目，写出始终为新格式

这部分会成为后续”增量压缩”和”fork / handoff”的基础。

### `agent-runtime`

负责把模型、工具、会话编排起来：

- 注册工具
- 追加用户输入
- 组装上下文视图
- 组装结构化模型续调上下文，而不是只依赖扁平 `role/content` 消息
- 对支持检查点续调的协议，优先从最近模型检查点之后只组装增量上下文
- 调用模型
- 记录代理输出
- 通过统一事件方法向多个订阅者分发运行时事件
- 记录工具调用与工具结果到会话磁带（通过 `TapeEntry` 工厂方法 + `with_run_id` 追踪）
- 在执行前强校验工具是否可用，禁止绕过禁用策略
- 单个用户 turn 在运行时内部按多步循环执行：模型 → 工具调用 / 结果 → 再回模型，直到本轮不再产生工具调用或达到内部步数上限
- 工具不可用、执行失败、结果错配会被收敛为结构化失败调用结果并落入磁带，供同轮后续模型步骤继续消费，而不是立即中止整个会话
- 工具相关运行时事件直接携带结构化调用/结果载荷，而不是仅传字符串
- 工具相关运行时事件进一步聚合为单个调用生命周期块，便于 TUI 直接渲染
- 整轮 turn 进一步聚合为轮次块事件，便于界面直接渲染完整时间线
- 整轮 turn 现在保留按发生顺序排列的块级序列（thinking / assistant / tool / failure），界面层不再只能依赖聚合字段猜测顺序
- 每个轮次块带稳定轮次标识与时间戳，便于时间线跳转与回放定位
- 轮次块只作为运行时事件发出，不再写入磁带（"derivatives never replace original facts"）
- TUI 从磁带 entries 按 `meta.run_id` 分组重建历史轮次，不依赖磁带内的 TurnRecord
- CLI 每轮会把原始 entries 同步落盘到 `.aia/session.jsonl`

第一阶段只实现最小 turn 执行，故意不把并发子代理一次做满，避免空壳化。

### `provider-registry`

负责本地 provider 管理：

- 保存 provider 档案
- 保存 provider 所属协议类型（Responses / OpenAI 兼容 Chat Completions）
- 记录当前活动 provider
- 从磁盘载入与写回 `.aia/providers.json`
- 保持 provider 持久化逻辑不泄漏进 `agent-cli`

### `openai-adapter`

负责首个真实模型提供商适配：

- 把内部统一请求映射为 Responses 风格 HTTP 请求
- 也支持映射为 OpenAI 兼容 Chat Completions 风格 HTTP 请求
- 把两类协议返回的文本、thinking 与工具调用统一还原为内部完成结果
- 在续调阶段按协议原生形态编码工具链路：Responses 使用 `function_call` / `function_call_output`，Chat Completions 使用 `assistant.tool_calls` / `tool.tool_call_id`
- Responses 还会持久化并回传 `previous_response_id`，避免后续 turn 重放整段历史
- 保持提供商细节停留在边缘层，不把外部协议泄漏进 `agent-core`

### `agent-cli`

这是一个**验证壳**，不是最终界面：

- 用最小入口证明核心库边界是可运行的
- 启动时提供最小终端内 provider 创建与选择流程
- 启动时提供最小终端内 provider 创建与选择流程，并允许显式选择协议类型
- 启动后提供最小可运行的 agent loop
- 在真实终端中优先运行最小 TUI，在非终端环境回退到文本 loop
- 入口已按 startup / provider / driver / loop / tui / model / error 拆分，避免单文件巨石继续膨胀
- TUI 启动状态机负责 provider 选择、provider 创建与首条问题输入
- 当前会话会记住上次使用的 provider 绑定（名称 / 模型 / 基地址，或 bootstrap）；用户在启动阶段通过 `F2` 才会替换
- 当前会话会记住上次使用的 provider 绑定（名称 / 模型 / 基地址 / 协议，或 bootstrap）；用户在启动阶段通过 `F2` 才会替换
- 当前最小 TUI 已把历史 replay 与本次运行分区显示，并支持焦点切换
- 当前最小 TUI 的消息区按单一时间线统一滚动，流式内容不再作为独立固定 footer 处理
- TUI 当前运行在主终端的内联视口中，不再切换到 alternate screen，也不再从左上角覆盖整个主屏内容
- 当前最小 TUI 通过后台驱动线程消费 loop 结果，渲染与 loop 执行已初步分离
- 文本 loop 与 TUI 已开始共用同一驱动协议，为后续 web / 其他客户端接入预留边界
- 共享驱动层已收敛为驱动本地错误与事件结果，不再直接泄漏命令行错误类型，也不再把错误提前压成字符串
- 共享驱动层在退出时只负责收尾与最终落盘，不再自动写入硬编码 handoff；handoff 保持为会话层显式能力
- 文本 loop 与 TUI 现在共享一致的失败策略：轮次失败会渲染状态与生命周期块，但不会直接打断整个交互会话
- 当前消息区已按单一时间线渲染 turn 流，并支持 Markdown 消息渲染、键盘与鼠标滚轮滚动、用户发送后的即时回显，以及底部固定流式状态栏
- `agent-cli` 已引入最小 theme 系统，当前默认主题为 `aura`；主题层集中提供用户消息、助手正文、thinking、工具块、分隔线与状态动画的语义样式
- TUI 消息渲染已组件化为 `MessagePanel`，内聚管理消息状态、滚动、两级缓存（`HistoryCache` + `OverlayCache`）与绘制逻辑；`TuiState` 不再直接持有消息渲染细节
- 下一步的 TUI 重构方向是继续拆出 Action / Reducer / Effect 状态机，避免 `tui.rs` 再次膨胀为同时承担状态机、IO 与排版的单文件巨石
- 未来可替换为真正的终端界面程序
- 桌面应用后续将共享同一运行时，而不是重写代理逻辑

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
- 最小可运行的 TUI 壳与模块化 CLI 结构

### 下一阶段直接承接的能力

- MCP 客户端 / 服务端桥接
- 统一工具规范向 Claude / Codex / MCP 的外部映射
- 真正的终端界面渲染循环
- 子代理调度
- 桌面壳
- 压缩策略与 fork

## 技术方向

第一阶段选用 Rust 工作区，原因：

- 终端界面和运行时都偏性能敏感
- 跨平台二进制分发稳定
- 后续接桌面壳时可以直接复用 Rust 核心

桌面层建议后续采用 Rust 核心 + Web 前端壳的方式，这样可以保留轻量分发与较低内存占用。

## 协议方向

- **对内**：维护一套统一工具规范，避免被单一模型厂商绑死
- **对外**：优先对接 MCP 这一类标准工具协议
- **适配层**：后续为不同模型工具规范增加边缘映射，而不是污染核心运行时
