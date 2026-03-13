# 项目状态

## 当前阶段

- 阶段：真实模型适配
- 当前步骤：真实模型适配继续推进：已补齐 OpenAI 兼容 Chat Completions 协议接入与 provider 协议记忆；主线焦点回到统一工具协议映射与 MCP 接入

## 已完成

- 建立 Rust 工作区
- 建立 `agent-core`
- 建立 `session-tape`
- 建立 `agent-runtime`
- 建立 `provider-registry`
- 建立 `openai-adapter`
- 建立 `agent-cli`
- 完成最小可运行验证
- 完成基础测试覆盖
- 完成项目级命名收敛
- 完成首个真实模型适配层接入
- 完成 `agent-cli` 启动时的 provider 创建与选择流程
- 完成 `agent-cli` 的最小可运行 agent loop
- 完成 `agent-runtime` 的统一事件流与多订阅者消费基线
- 完成 `session-tape` 的类型化事实磁带、handoff 事件与 anchor-based 重建
- 完成工具调用与工具结果写入会话磁带，并进入统一事件流
- 完成工具调用与工具结果的稳定调用标识贯穿，并收紧禁用工具执行策略
- 完成工具调用生命周期事件聚合，CLI 已优先消费聚合事件块
- 完成 turn 轮次块事件聚合，CLI 已优先渲染完整轮次
- 完成 turn 稳定轮次标识与时间戳贯穿，CLI 已显示轮次元数据
- 完成 `.aia/session.jsonl` 会话索引落盘，轮次块已可重放载入
- 完成 git 目录初始化
- 完成 agent-cli 模块拆分与最小 TUI 接入
- 完成 TUI 启动状态机，provider 选择与首条问题输入已迁入 TUI
- 完成会话级 provider 绑定记忆与启动阶段主动替换策略
- 完成最小 TUI 的回放面板、本次运行分区、焦点切换与快捷键提示
- 完成 TUI 后台驱动线程接入，发送消息不再阻塞整个界面
- 完成共享 driver 模块抽取，文本 loop 与 TUI 已开始共用统一驱动接口
- 完成共享 driver 结果与错误边界收敛，不再直接暴露命令行错误类型或字符串化错误
- 完成退出阶段的共享 driver 语义收敛：只做 finalize 与持久化，不再自动注入硬编码 handoff
- 完成 TUI 消息区的 Markdown 渲染与基于视口高度的滚动收敛
- 完成 TUI 中 thinking 区块与正式回答区块的视觉分层增强
- 完成 TUI 工具调用块、输入提示符与状态栏分隔样式优化，消息区层次更清晰
- 完成用户消息背景高亮与 thinking 单行内联展示，继续压缩消息阅读负担
- 完成移除 Assistant 标题、thinking 内联 Markdown 渲染与用户整块消息背景收口
- 完成 thinking 首行内联但保留 Markdown 后续换行，避免内容被强制压平
- 完成移除用户消息中的 `You` 标签与补全式气泡填充，背景仅跟随真实消息内容
- 完成用户消息气泡的轻量左右内边距，背景范围稍增但不再显得生硬
- 完成最小 theme 系统落地，并以 aura 作为第一套 TUI 主题配色
- 完成 aura 主题下的正文与工具 Markdown 适配，并补上用户消息即时回显与消息列表内运行状态
- 完成消息列表鼠标滚动、底部固定流式状态栏与更平滑的 Aura 状态亮暗动画
- 完成消息列表基于最新视口信息的自动跟底修复，并把流式轮次与历史消息的间隔提升为双空行
- 完成 stream turn 状态文案去除省略号、统一块级轻量 padding，以及消息区与输入框之间的固定垂直间距
- 完成 `tui_markdown` 与 `tui_timeline` 模块抽取，开始收敛 TUI 纯逻辑边界
- 完成消息视图 render cache 首版接入，spinner 动画不再触发整段时间线重建
- 完成消息区段落空行保留、统一左侧起始间距与更强的自动跟底修复，继续收敛 TUI 可读性与流式稳定性
- 完成对外产品名与本地隐藏目录从 `like` 统一重命名为 `aia`
- 完成 `session-tape` 的命名锚点、查询切片、命名磁带存储抽象与 fork / merge 语义补齐
- 完成 `.aia/session.jsonl` 旧格式兼容门面保留，避免当前 CLI 会话文件被隐式迁移
- 完成 `session-tape` TapeEntry 扁平化为 `{id, kind, payload, meta, date}`，对齐 republic 数据模型
- 删除 SessionFact 枚举、SessionMetadata、SessionEvent、TurnRecord、ToolInvocationRecord 等旧类型
- 简化 Anchor 为 `{entry_id, name, state: Value}`，不再硬编码 phase / summary / next_steps / owner
- 运行时不再将 TurnRecord 写入磁带，遵循 "derivatives never replace original facts"
- TUI 改为从 entries 按 `meta.run_id` 分组重建历史轮次
- 旧格式 JSONL 载入时自动转换为扁平 entry，写出始终为新格式
- 完成 TUI 消息面板组件化（`MessagePanel`），将消息渲染状态与行为从 `TuiState` 中内聚提取
- 完成两级渲染缓存：`HistoryCache`（仅在 turn 完成或 resize 时失效）+ `OverlayCache`（仅在新 delta 到达时失效），流式期间每帧成本从 O(N × markdown_parse) 降至 O(hash) + O(流式增量)
- 完成每帧 delta 上限（`MAX_DELTAS_PER_FRAME = 64`），防止突发 delta 阻塞渲染
- 完成流式状态标签统一为英文现在分词：Waiting / Thinking / Generating
- 完成 `agent-runtime` 从单次模型调用收敛为单轮内多步循环，支持模型 → 工具 → 再回模型
- 完成工具不可用、工具执行失败、工具结果错配的非终止式收敛：失败结果会写入磁带并回灌给同轮后续模型步骤
- 完成文本 loop 与 TUI 的失败策略统一：当前轮失败会显示状态，但不会直接结束整个会话
- 完成历史轮次重建策略修正：回放优先显示最后一条助手消息，并合并同轮 thinking 片段
- 完成 `openai-adapter` 对 OpenAI 兼容 Chat Completions 协议的普通与流式支持
- 完成 `provider-registry` 的 provider 协议类型区分，并支持 Responses / Chat Completions 双协议 provider
- 完成 TUI 与文本 provider 创建流程的协议选择接入
- 完成会话级 provider 绑定带上协议字段，避免同地址同模型的不同协议恢复错配
- 完成本地 `Minum-Security-LLM` provider 以 Chat Completions 协议写入 `.aia/providers.json`
- 完成 TUI 超长消息展示截断：历史与流式消息过大时仅渲染前若干字符，并明确提示已截断
- 完成运行时同轮重复工具调用防重：相同工具与相同参数在同一轮内重复出现时会被跳过，并提示模型直接基于已有结果继续
- 完成结构化续调上下文：工具调用与工具结果不再只压平成普通消息，而是以结构化条目贯穿 `agent-core` / `session-tape` / `agent-runtime` / `openai-adapter`
- 完成双协议工具续调映射：Responses 走 `function_call` / `function_call_output`，Chat Completions 走 `assistant.tool_calls` / `tool.tool_call_id`
- 完成 Responses 检查点续调：成功响应会记录模型检查点，后续 turn 通过 `previous_response_id` + 增量输入继续远端响应链
- 完成 TUI 消息列表统一滚动：流式内容与状态行动画已并入主消息时间线，不再只有局部 overlay / footer 在动
- 完成 turn 顺序块渲染：thinking / tool / assistant / failure 按真实发生顺序进入 `TurnLifecycle.blocks` 并驱动历史与当前消息渲染
- 完成 TUI 内联视口模式：不再切换到 alternate screen，且通过 `Viewport::Inline` 在当前终端输出下方保留固定渲染区域，避免覆盖 shell 提示内容

## 正在进行

- 统一工具规范向外部协议的映射收敛，并继续把共享 driver 往客户端无关边界推进；运行时语义与双协议模型适配已收稳，现在没有必要跳过协议层直接堆完整界面
- 统一工具规范向外部协议的映射收敛，并继续把共享 driver 往客户端无关边界推进；当前已补齐协议原生的工具续调主链，后续重点回到更完整的 MCP 接入与工具协议映射
- 统一工具规范向外部协议的映射收敛，并继续把共享 driver 往客户端无关边界推进；当前已补齐协议原生的工具续调主链与 Responses 检查点续调，后续重点回到更完整的 MCP 接入与工具协议映射
- TUI 重构阶段 A（止血）已基本完成；阶段 B（拆状态机：Action / Reducer / Effect）作为次优先穿插推进，不压过协议主线

## 下一步

1. 把统一工具规范往外映射到标准协议方向
2. 推进 MCP 风格工具协议接入
3. 基于新的多步运行时语义，为后续真实工具与协议桥接补齐更细的结果分类与限制策略
4. 继续把运行时与客户端层接到更完整的命名磁带能力，但不破坏现有兼容门面
5. 按 `docs/tui-plan.md` 推进阶段 B（拆状态机：Action / Reducer / Effect），穿插在协议主线之间
6. 继续收敛共享 driver、启动编排与 TUI reducer 边界，减少 `app.rs` 与 `tui.rs` 的重复职责
7. 在协议边界更稳之后，再把当前最小 TUI 扩展为更完整的终端界面

## 为什么当前先不直接做完整界面

因为界面依赖稳定的运行时、会话模型和工具协议；现在虽然磁带核心已经补齐，但如果跳过工具协议映射与 MCP 接入直接堆界面，仍会把后续协议边界和客户端职责锁死。当前 TUI 重构也只先做边界收敛和性能止血，不把它提升为压过协议层的主线。

## 阻塞

- 当前无硬阻塞
