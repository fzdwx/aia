# 项目状态

> 本文件只保留 **当前真实状态**。详细历史看 `docs/evolution-log.md`，未完成事项看 `docs/todo.md`。

- Last verified: `2026-03-31`
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
- provider / session settings 外层语义已开始按 `ProviderAccount + AdapterKind + ModelRef` 收口：provider 列表与设置面板使用 `id / label / adapter`，session 设置使用 `model_ref + reasoning_effort`
- server 内部活动选择快照也已改成 `provider_id / model_id` 命名，避免继续在桥接层传播旧的 `name / model` 语义

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
- `RFC 0005 Provider Domain Redesign`：**Draft**
- `RFC 0001 Question Runtime Tool`：**Accepted**
  - 说明：pending question 的共享类型、server 控制面和 Web 承接已存在
  - 但“通用 suspend/resume 原语完全定稿并宣告收口”这件事还没有被提升到 `Implemented`
  - 另外，重启后 orphaned pending question 当前会被 hydrate 路径显式记为 cancelled，而不是恢复成可继续回答的 active pending 状态

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
4. 观察 `Question` 控制面在真实 provider / 多步工具链下的边缘稳定性，并明确 RFC 0001 的完成线

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
