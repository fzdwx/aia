# 需求说明

> 本文件只描述 **项目目标、边界与非目标**。当前进度看 `docs/status.md`，历史过程看 `docs/evolution-log.md`。

- Last verified: `2026-03-30`

## 愿景

做一个正常、好用、性能克制、跨平台的 agent harness。

它应当：

- 以 Web 工作台作为当前主承接面
- 以共享 Rust 运行时作为真正核心
- 保持桌面壳、CLI、自定义客户端都能复用同一条运行时主链
- 不为了短期堆功能破坏长期边界

## 核心需求

### 1. 交互承接

- 提供一个好用的 Web 界面
- 后续可接入桌面壳
- 支持 Windows、Linux、macOS
- 当前主承接形态是 `apps/web` + `apps/agent-server`
- 除 Web 外，还应支持少量直接 CLI 驱动入口，例如 `self` 模式

### 2. 运行时控制面

- `apps/agent-server` 必须保持为 canonical runtime control surface
- server 不应只服务单一网页，而应继续可被其他客户端驱动
- bootstrap 阶段应允许嵌入方覆写应用级默认值，例如请求超时、system prompt、runtime hooks
- run 阶段应显式配置监听地址，而不是把 bind 地址混进 bootstrap 状态装配

### 3. 会话模型

- 会话事实必须保持 append-only
- 会话主事实以 session tape 为准，派生状态不能覆盖源事实
- 会话需要支持：
  - 多 session
  - 恢复 / 重建
  - handoff / anchor
  - fork / merge
  - session 级模型与思考等级
  - 标题、最近活跃时间等元信息投影
- session 元信息与会话事实需要分层持久化：
  - 结构化元信息适合 SQLite
  - 轮次与工具事实保持 jsonl tape

### 4. 工具与协议

- 内部只维护一套统一工具协议
- 工具名与工具契约应保持短名、稳定、与底层执行器解耦
- 对外兼容 Claude / Codex 风格工具规范
- 在共享协议边界稳定后，继续推进 MCP 接入
- 内建工具默认可用，但应保持可控、可裁剪，不与某个模型家族强耦合

### 5. 代理能力

- 感知不同模型的人格差异
- 支持工具调用、上下文压缩、handoff、trace、channel 承接
- 取消 / stop 语义需要贯穿 server、runtime、provider streaming 与工具执行路径
- 允许通过 structured question 等控制面在合适承接面里做交互，但不能把交互逻辑散落到各客户端各自实现

### 6. 用户体验

- Web 界面要保持流畅、低闪烁、低阻塞
- 流式体验需要稳定，不应频繁出现布局跳动和状态漂移
- session 列表、历史恢复、当前轮次恢复、消息队列、interrupt/cancel 都应具备清晰一致的表现
- 配置面板、trace 工作台、channel 工作台都应在同一信息架构下继续收稳

### 7. 性能与可靠性

- 不以跑分最大化为目标
- 不能在 CPU 和内存占用上走极端
- 生产路径不能依赖 panic
- 运行时与存储边界要清楚，避免 session tape、snapshot、SQLite 状态互相漂移

### 8. 可观测性

- 本地 trace 需要能还原 agent loop、LLM 请求与工具执行关系
- 压缩日志需要能独立查看，而不是混进普通对话请求
- trace 概览接口要有真实分页与可复用的本地聚合快照，不能靠每次全表重算

## 当前阶段边界

当前阶段关注的是：

- Web 工作台
- `agent-server` 控制面
- 共享 runtime / tools / store / trace 主链
- channel 接入的桥接边界

当前阶段**不要求**：

- 完整桌面壳
- 大规模 provider 扩展
- 完整 OTLP exporter / collector 集成
- 异步子代理调度

这些方向可以做，但不应抢在当前主链收口之前。

## 当前阶段的完成判断

当前阶段是否做得对，主要看这些条件：

1. Web、server、runtime、store 之间的 ownership 清晰
2. session 事实、元信息、快照与 SSE 投影不会互相漂移
3. stop/cancel、queue、history restore、current-turn restore 等主路径稳定
4. 工具协议保持内部统一，对外映射边界清楚
5. trace、compression、channel、session settings 这些控制面不再各自长出第二套语义

## 非目标

以下内容不是本阶段的主目标：

- 为了“支持更多平台”而复制第二套 agent 逻辑
- 在共享协议未稳定前大量扩展外部协议特化分支
- 为了短期方便把业务规则重新塞回 app 壳
- 用临时文档流水账代替稳定的需求、架构与状态分层

## 与其他文档的关系

- `docs/status.md`：回答“现在做到哪了”
- `docs/architecture.md`：回答“边界和 ownership 怎么分”
- `docs/todo.md`：回答“接下来还没做什么”
- `docs/rfc/*`：回答“为什么这样设计”
