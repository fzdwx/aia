# 待办清单

> 这里只跟踪 **未完成事项**。已完成事项不再堆在这里，请分别看：
>
> - `docs/status.md`：当前状态
> - `docs/evolution-log.md`：历史过程
> - `docs/rfc/`：设计决策

- Last reviewed: `2026-03-30`
- 状态说明：`doing` / `todo` / `research` / `icebox`

## P0｜当前主线

### 1. `agent-server` runtime ownership / return-path 收口

- 状态：`doing`
- 重点：继续压缩 `apps/agent-server/src/session_manager/turn_execution.rs` 中的 `runtime.take() -> worker -> RuntimeReturn` 主线
- 目标：让 `apps/agent-server` 更像稳定驱动面，而不是持续膨胀的 app 壳
- 验证关注：turn / cancel / history / current-turn / pending question / channel 入口回归

### 2. 统一工具协议的对外映射与 MCP 接入

- 状态：`todo`
- 重点：在保持内部工具短名、稳定、与执行器解耦的前提下，对接外部模型协议与 MCP
- 目标：避免继续把外部差异反向污染共享核心层

### 3. stop/cancel 覆盖率补强

- 状态：`todo`
- 重点：
  - 长时间 shell pipeline
  - 慢 provider streaming
  - 连接建立 / TLS / 代理缓冲等窗口
- 目标：把取消语义从“主链可用”继续推进到“复杂场景也稳定”

## P1｜近期事项

### 4. Feishu 生产边界补强

- 状态：`todo`
- 重点：mention gate、群权限策略、白名单、可用范围控制

### 5. `Question` 控制面与 RFC 0001 完成线

- 状态：`todo`
- 重点：
  - 继续观察 pending question 在真实 provider / 多步工具链中的边缘态
  - 明确 RFC 0001 从 `Accepted` 升到 `Implemented` 还缺什么
  - 判断是否要把“重启后 orphaned pending question 重新开放为 active 状态”补成正式语义，还是继续保持 hydrate 时自动 cancelled 的保守策略
- 注意：不要把当前控制面可用，误写成“通用 suspend/resume 已完全收口”

### 6. Trace 数据模型继续收口

- 状态：`todo`
- 重点：从当前 span store + event timeline 继续推进到 richer events / resources 形态
- 约束：不抢在 MCP 与工具协议映射之前做 exporter / collector 集成

### 7. 文档第二轮整理

- 状态：`doing`
- 重点：
  - 继续整理 `docs/evolution-log.md` 中最容易误读的 `未提交 / 待本轮执行 / 历史阶段计划` 条目
  - 审核次级文档与 RFC 正文里的过期实现叙述，特别是 `async-phases.md`、`frontend-web-guidelines.md`、`rfc/README.md`
  - 评估 `async-phases.md`、`generative-ui-article.md` 等文档是否应进入 `archive / notes` 分层
  - 让新文档结构长期保持“README / status / requirements / architecture / todo / evolution-log / rfc”职责分离

## P2｜后续事项

### 8. 桌面壳接入

- 状态：`todo`
- 目标：复用现有 Web 前端与 Rust 核心，而不是复制第二套 agent 逻辑

### 9. Topic Recall / 主题编织

- 状态：`research`
- 说明：保留为研究项，不作为当前主线

### 10. Generative UI / Widget 系统

- 状态：`research`
- 说明：保留设计研究，不抢在主链稳定前展开实现

## Icebox｜明确不在当前阶段

- 状态：`icebox`：完整 OTLP exporter / collector 集成
- 状态：`icebox`：大规模新增 provider 家族
- 状态：`icebox`：异步子代理调度

## 已从本文件移出的问题

以下内容已经不再作为 TODO 挂在这里：

- 上下文自动压缩：已完成
- 前端 tool 展示居中问题：已完成
- “是否需要支持多个会话”：主链已支持，多会话不再是待确认问题

后续如果再有新 backlog，直接按优先级补到这里，不要再把“已完成事项 + 历史分析 + 草稿问题”混写在同一个文件里。
