# 项目状态

> 本文件只保留 **当前真实状态**。详细历史看 `docs/evolution-log.md`，未完成事项看 `docs/todo.md`。

- Last verified: `2026-04-08`
- Current source of truth: 以当前代码与 RFC 头部状态为准；本文件负责做压缩后的事实对齐

## 当前阶段

- 阶段：**Web 界面实现**
- 当前细分步骤：**Web 界面 ↔ 运行时桥接收口**
- 下一优先方向：
  1. 继续压缩 `apps/agent-server` 的 runtime ownership / return-path 复杂度
  2. 推进统一工具协议的对外映射与 MCP 接入
  3. 继续补强 stop/cancel、channel 边界与 trace 诊断稳定性

## 当前已确认事实

### 1. 主承接面

- `apps/web` 是当前主客户端工作台
- `apps/agent-server` 是当前 canonical runtime control surface，而不是只服务单一前端页面的薄后端
- `agent-server` 同时提供：
  - HTTP + SSE server
  - `self` CLI 模式
  - 可嵌入的 `bootstrap_state_with_options(...)` / `run_server_with_options(...)` façade

### 2. 共享核心结构

当前工作区已稳定围绕这些核心 crate 组织：

- `aia-config`
- `agent-core`
- `agent-prompts`
- `agent-runtime`
- `session-tape`
- `builtin-tools`
- `agent-store`
- `openai-adapter`
- `provider-registry`
- `channel-bridge`
- `channel-feishu`
- `channel-weixin`
- `weixin-client`

### 3. 持久化形态

- `.aia/store.sqlite3`：session 元信息、provider、channel、trace 等结构化数据
- `.aia/sessions/*.jsonl`：每个 session 一个 append-only tape
- session 元信息与 tape 事实已明确分层：
  - 元信息走 SQLite
  - 轮次与工具事实走 jsonl tape

### 4. Web 主路径

`apps/web` 当前已经具备：

- session 列表 / 创建 / 切换 / 删除
- 历史与 current-turn 恢复
- SSE 流式消息、工具时间线、错误与重同步处理
- session 级模型 / 思考等级设置
- Settings / Providers / Channels / Trace 工作台
- 基于 `streamdown` 的统一 Markdown 渲染
- SSE 文本增量合帧、`streamdown` block-level tail update 与超大流式 Markdown 降级，降低长 thinking / 大文本块场景下的前端内存与重渲染压力
- provider streaming 自动重试在最终失败时会保留最后一次真实 HTTP 错误上下文，不再把 `502` 等上游状态码覆盖成泛化的“重试次数已耗尽”
- Web 端 `tool_output_delta` 的流式投影现已对 `output` 与单段 `outputSegments` 做尾部窗口裁剪，并避免滚动跟随逻辑在每个 delta 上全量拼接所有 segments，降低长 shell / 大 widget HTML 输出场景下的前端内存与 GC 压力
- Web SSE 接入层现已把相邻同 `session/turn/invocation/stream` 的 `tool_output_delta` 按帧合并后再派发，进一步降低长工具输出场景下的 store 更新频率与 React 重渲染压力
- `apps/agent-server` 的 runtime return-path 现已避免 queued message 在二次 `SubmitQueuedMessages` 调度失败时被提前 `drain` 丢失；失败时会保留队列并复位 `queue_processing`

### 5. Session 能力

以下能力已落地并贯通到前后端：

- session 元信息字段：`title_source`、`auto_rename_policy`、`last_active_at`
- `session_updated` SSE 投影
- 自动重命名服务
- sidebar 标题动画与最近活跃时间显示
- 消息队列与 interrupt / cancel 主链（运行中即时入队目前以内存队列 + SSE 为主，崩溃恢复只覆盖已落盘 queue 事件）
- session 级 provider/model/reasoning 设置

### 6. RFC 状态

按 RFC 文档头部与代码现状核对：

- `RFC 0002 Session Auto Rename`：**Implemented**
- `RFC 0003 Scroll Position Anchor on History Load`：**Implemented**
- `RFC 0004 Message Queue and Interrupt Mechanism`：**Implemented**
- `RFC 0001 Question Runtime Tool`：**Accepted**
  - 说明：pending question 的共享类型、server 控制面和 Web 承接已存在
  - 但“通用 suspend/resume 原语完全定稿并宣告收口”这件事还没有被提升到 `Implemented`
  - 另外，重启后 orphaned pending question 当前会被 hydrate 路径显式记为 cancelled，而不是恢复成可继续回答的 active pending 状态
- `RFC 0005 LLM Automatic Retry`：**Implemented**
  - 说明：已在 `crates/openai-adapter` 落地 provider-backed LLM 自动重试；仅覆盖首个可见流式增量前的发送 / HTTP / 早期流读取失败，不扩展到 Web 请求层、通用工具执行或 mid-stream 已见输出后的失败

### 7. Channel 与外部承接

- Feishu transport 已接入正式长连接 worker
- Weixin transport 已接入 catalog、profile 配置与运行态主链
- Web 已具备微信扫码登录控制面
- channel 持久化已统一进入 SQLite，而不是继续维护独立 registry 文件

### 8. Trace 与可观测性

当前已具备：

- 本地 trace / overview / dashboard
- agent loop 级聚合
- compression 日志独立视图
- `sync_required` 事件与前端重拉恢复
- loop、token、失败态、session activity 等聚合读路径

### 9. Async 主链结论

异步化阶段本身已经完成。

当前结论是：

- provider I/O、tool 执行、runtime turn loop、server session manager 都已切到 async 调用面
- 后续重点是 **简化 ownership / return-path 与共享层继续收口**
- 当前不再把“继续 async 化”作为主目标本身

## 当前工作重点

1. 收口 `apps/agent-server/src/session_manager/turn_execution.rs` 里的 `runtime.take() -> worker -> RuntimeReturn` 主线
2. 统一内部工具协议向外部模型协议映射，并开始 MCP 接入
3. 补强 stop/cancel 在长 shell、慢 provider streaming、复杂上游网络窗口下的真实覆盖率
4. 继续补强 Feishu 的 mention / 权限 / 白名单等生产边界
5. 继续推进 trace 的 richer events / resources 形态，但不抢在 MCP 与工具映射之前

## 下一步

1. 优先继续收口 server 驱动面与宿主边界，而不是继续扩张客户端表层能力
2. 在工具协议边界稳定的前提下推进 MCP 接入
3. 继续下沉可复用驱动辅助，降低其他客户端复用 `agent-server` 的成本
4. 在不偏离当前阶段主线的前提下，继续推进 widget host 协议收口：先把共享协议与宿主边界做实，再按需补 sandbox/CSP、ErrorBoundary 与 capture/export
5. 观察 `Question` 控制面在真实 provider / 多步工具链下的边缘稳定性，并明确 RFC 0001 的完成线

## 暂时不做

- 桌面壳完整实现
- 完整 OTLP exporter / collector 集成
- 大规模新增 provider 家族
- 异步子代理调度

## 风险与注意事项

- `docs/status.md` 必须继续保持“短、准、当前态”；历史细节不要再堆回这里
- `apps/agent-server/src/session_manager/turn_execution.rs` 仍是当前实现复杂度热点
- `RFC 0001` 不应被误写成“已完全实现”；当前是控制面与承接面可用，但抽象收口尚未正式完成
- 桌面壳尚未开工；当前跨平台方向仍以 Web + server 复用为主
- `WidgetRenderer` 当前已具备参数流式预览、stable iframe、current-turn 恢复，以及前后端贯通的共享 widget bridge 协议层：前端已把 `render` / `theme_tokens`、`ready` / `scripts_ready` / `resize` / `error` / `send_prompt` / `open_link` 收敛成协议辅助层，并继续保留 `aia-widget-*` 兼容别名
- runtime / server 已开始把 widget bridge 作为一等 `stream` 事件承载：`WidgetHostCommand::Render` 会随流式工具事件显式广播，前端宿主也会通过 `POST /api/session/widget-event` 把 `WidgetClientEvent` 回传到 server 并转成显式 SSE `stream` 事件
- widget bridge 事件已进入 session tape 主链：运行中的 host/client 事件会先缓存在 session slot，turn 返回后统一刷入 tape；空闲态的 `POST /api/session/widget-event` 会直接写入 tape
- current-turn 重建已开始消费显式 `widget_host_command`：带 `run_id` 的 `render` 事件可在 `rebuild_session_snapshots_from_tape(...)` 中恢复到对应 tool block 的 `widget` 投影，用于 hydrate 当前未完成轮次
- completed-turn 历史模型已开始携带一等 `replay_events`：`ToolInvocationLifecycle` 现在会保留 `widget_host_command` / `widget_client_event` 序列，前端 completed tool row 也会消费其中的 render 事件恢复 widget 预览
- 当前前端已把 `turn_id` 传到 completed `WidgetRenderer` 的宿主上报路径，已完成轮次的 widget 交互可以继续回传到 server；是否还要扩展到历史页以外的更多宿主入口，后续再按 turn 级上下文继续收口
- 运行中的 widget 事件已开始通过 runtime 持有者侧入口即时排空：server 会在流式 delta 与显式通知到达时驱动 runtime 直接 `append_tape_entry(...)`，不再只依赖 turn 返回时的统一刷盘
- turn 返回时的缓冲刷盘仍然保留为兜底路径；后续若要继续收口，可以考虑把这条 runtime-side drain 进一步抽成更通用的 per-session runtime hook，而不只是 widget 专用入口
- sandbox/CSP、ErrorBoundary、CSS bridge 深化与 capture/export 仍是后续工作，但不应抢在当前 bridge/tool 协议/MCP 主线之前扩成 dashboard 产品层
