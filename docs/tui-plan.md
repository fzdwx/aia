# tui plan

## 目标

TUI 重构先解决四件事：

1. 把当前单文件巨石拆成可维护的边界
2. 把全量重排版、全量重绘改成增量更新
3. 继续坚持 TUI 只是运行时事件流的订阅者，而不是反向侵入 runtime
4. 为后续工具协议映射、MCP 接入和更完整界面留出稳定壳层

## 当前问题

### 维护性

- `apps/agent-cli/src/tui.rs` 已超过 2500 行，同时承载启动状态机、输入编辑、滚动、驱动轮询、历史轮次重建、Markdown 渲染、主题应用和各阶段布局绘制
- `Phase` 既表达业务流程，也隐含输入语义和布局切换，导致改一个阶段时往往要同时碰状态、事件处理和渲染
- `TuiState` 直接保存大量 UI 细节和业务细节，难以判断哪些字段属于 domain，哪些字段只是 view cache
- 文本 loop 与 TUI 虽然已共用 driver，但 TUI 仍然持有较多启动编排和 turn 提交细节，客户端边界还不够干净

### 性能

- 主循环固定 `100ms` 轮询，即使没有输入、没有新事件也会持续 `draw`
- `draw_messages -> message_lines -> turn_lines -> markdown_lines` 会在每一帧重新遍历全量历史轮次并重新解析 Markdown
- 流式输出时每个 delta 都会让整个消息列表重新拼装，而不是只更新当前 streaming turn 的 render cache
- 历史轮次重建和视图排版没有分层缓存，窗口尺寸变化、滚动、spinner 动画都会触发同一条重路径

## 重构原则

- 保持 `agent-runtime` 继续只发统一事件流，TUI 只消费事件，不新增旁路协议
- 启动流程、聊天会话、渲染缓存分别建模，不再继续扩张单一 `TuiState`
- 优先做 render cache、脏区标记和事件归并，再考虑视觉功能扩展
- 可以引入异步，但异步只用于事件汇聚、后台任务和 IO 解耦，不把它当成替代渲染优化的万能药
- 复用现有 driver 抽象，不把 provider 持久化、session 落盘再塞回视图层
- 每一步都保持当前 CLI 可运行、测试可持续补齐，不做一次性大爆炸迁移

## 目标分层

### 1. `tui::app`

- 只负责 TUI 顶层编排
- 管理终端初始化、退出恢复、主循环和 resize / input / tick / driver 事件汇总
- 不直接写具体布局细节和 Markdown 排版

### 2. `tui::state`

- 只保存稳定 UI 状态
- 拆分为 `StartupState`、`ChatState`、`InputState`、`ScrollState`、`PanelsState`
- 运行时事件进入 reducer 后更新状态，不在绘制阶段临时推导业务结果

### 3. `tui::action` + `tui::reducer`

- 把键盘、鼠标、driver delta、turn 完成、tick 统一成内部 action
- 用 reducer 明确状态转移，替代当前分散在多个 `handle_*` 函数里的隐式副作用
- 区分“更新状态”和“触发 effect”，例如提交 turn、切换 provider、保存绑定

### 4. `tui::effects`

- 封装与外部世界交互的动作
- 包括 driver 提交、provider registry 写入、session provider binding 持久化
- reducer 返回 effect，顶层 app 执行 effect，避免状态更新函数直接穿透到 IO
- 若后续切到 async，这一层是最适合承接 `spawn`、取消、超时与后台任务汇总的位置

### 5. `tui::timeline`

- 专门负责把 `TurnLifecycle`、streaming delta、历史 replay 条目整理成视图模型
- 输出稳定的 `TimelineItem` 列表，例如 user / thinking / tool / assistant / failure / status
- 后续工具协议增加更多事件类型时，只在这里扩展映射

### 6. `tui::render`

- 只负责把视图模型渲染为 ratatui widget / line buffer
- 再拆为 `layout.rs`、`messages.rs`、`input.rs`、`startup.rs`、`logs.rs`
- 主题仍从 `theme.rs` 提供语义样式，render 层不自己发明配色

### 7. `tui::markdown`

- 把当前 Markdown 解析与 terminal line 构建独立出来
- 对同一段内容按 `(content_hash, style_kind, width_class)` 做缓存
- 让消息渲染复用缓存结果，而不是每帧重复 parse

## 性能优先级

### 第一优先

- 引入脏标记：只有输入变化、scroll 变化、窗口尺寸变化、stream delta、turn 完成时才重建消息视图
- 把 spinner tick 改成仅更新 footer / status 区域，不重新生成整条时间线
- 给历史 turn 和 streaming turn 分别建 render cache
- 保持渲染主线程尽量同步且短路径，先减少单帧工作量，再考虑是否需要 async render feeder

### 第二优先

- 把固定 `100ms poll + draw` 改成事件驱动加低频 tick
- 无新事件时降低 redraw 频率；有 streaming 时才开启较高频 tick
- 将日志面板和消息面板拆分缓存，避免开关 logs 时重算整个消息列表
- 若改为 async，优先把输入事件、driver delta、tick 合流成单一 channel，而不是在多个任务里直接抢状态锁

### 第三优先

- 为历史 replay 建立 `TimelineCache`，仅在会话载入或新 turn 完成后追加
- 宽度变化时只做必要的换行重排，不重复做业务层 turn 归并
- 若 ratatui 当前 buffer diff 已足够，则避免手工做更重的虚拟 DOM；先用简单 cache 收敛热点

## 建议实施顺序

### 阶段 A：止血 ✓

已完成：

- 抽出 `tui_markdown.rs` 和 `tui_timeline.rs` 模块
- 消息面板组件化为 `MessagePanel`，将消息渲染状态与行为从 `TuiState` 中内聚提取
- 引入两级渲染缓存：`HistoryCache`（仅在 turn 完成或 resize 时失效）+ `OverlayCache`（仅在新 delta 到达时失效）
- 流式期间每帧成本从 O(N × markdown_parse) 降至 O(hash) + O(流式增量)
- 增加每帧 delta 上限（`MAX_DELTAS_PER_FRAME = 64`），防止突发 delta 阻塞渲染
- spinner tick 不再触发整段时间线重建
- 所有现有测试通过，仅字段访问路径更新

### 阶段 B：拆状态机

- 引入 `Action` / `Reducer` / `Effect`
- 将 provider 选择、provider 创建、首条问题、聊天阶段的输入处理从绘制和 IO 中拆出
- 让 `run_tui_loop_inner` 缩成“收事件 -> dispatch -> 执行 effect -> 按需 redraw”

### 阶段 C：拆渲染

- 将聊天区、输入栏、启动页、日志面板分文件
- 让 render 层只消费只读视图模型，不直接修改状态
- 把 `FocusArea`、scroll 计算、footer 状态都收束到统一的 view model

### 阶段 D：补观测与扩展点

- 增加简单性能计数：每秒 redraw 次数、Markdown cache 命中率、timeline rebuild 次数
- 为 MCP / 工具协议事件预留扩展的 `TimelineItemKind`
- 在不破坏最小验证壳定位的前提下，再考虑更完整终端界面

## 关于异步

异步值得考虑，但只应放在合适的位置：

- 适合异步的部分：driver 事件接收、provider 持久化、会话落盘、日志汇聚、未来 MCP 工具 IO
- 不适合优先异步化的部分：Markdown 排版、消息列表拼装、scroll 计算、当前帧渲染路径
- 原因很直接：现在的主要卡顿来自“每帧做了太多工作”，不是“主线程缺少并发”

如果后续引入 async，建议采用这条边界：

- UI 主循环仍保持单线程 reducer 模式，避免共享状态到处加锁
- 后台任务把结果发回统一 action channel，由 app 统一 dispatch
- 不让渲染层直接持有 async runtime handle，也不让 widget 绘制过程中等待 future
- 继续保留当前 driver 的客户端无关边界；必要时把 thread 版 driver 演进为 async driver，但协议不变

换句话说，async 可以是第二刀，但第一刀仍然应该是缓存、脏标记和事件驱动 redraw。

## 验收标准

- `tui.rs` 不再继续增长为单文件巨石，主文件只保留顶层编排
- 连续 streaming 时 CPU 占用和输入延迟明显下降，空闲时不再高频 redraw
- 新增一个时间线事件类型时，不需要同时修改主循环、状态机和消息绘制三处以上
- 文本 loop 与 TUI 继续共用 driver，未新增客户端旁路协议

## 与当前阶段的关系

- 当前项目主线仍然是工具协议映射与 MCP 接入，这个优先级不变
- TUI 重构应优先做“边界收敛 + 性能止血”，而不是直接扩功能
- 这样做的原因是：如果先把界面做厚，再补协议层，TUI 很快会再次绑死运行时边界

## 明确延后

- 动画细节打磨
- 多窗口桌面联动
- 异步子代理的复杂可视化
- 大规模视觉改版

先把 TUI 从“可跑但持续膨胀”收敛成“可拆、可测、可增量更新”的观察面板，再继续扩展界面能力。
