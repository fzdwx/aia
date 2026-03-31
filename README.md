# aia

`aia` 是一个以 **Rust 共享运行时 + Web 工作台** 为核心的 agent harness。

它当前的主承接面是：

- `apps/web`：主界面
- `apps/agent-server`：规范化运行时控制面（HTTP + SSE + CLI self 模式）
- `crates/*`：共享核心能力，例如 session tape、runtime、store、tools、provider / channel 适配

项目当前重点不是再造第二套壳，而是继续把 **Web、server、runtime、tools、trace** 这条主链收稳，并为后续 **统一工具协议对外映射** 与 **MCP 接入** 做准备。

## 当前已经具备

- Web 聊天工作台：session 列表、历史、当前轮次恢复、流式消息、Trace、Settings、Channels
- `agent-server` 控制面：HTTP API、SSE 事件流、`self` CLI 模式、可嵌入 bootstrap / run façade
- append-only session tape：handoff、anchor、fork / merge、恢复、重建
- session 元信息持久化：SQLite 中维护 session / provider / channel / trace 元数据
- session 级模型与思考等级设置
- session 自动重命名、最近活跃时间投影、标题动画
- 消息队列与 interrupt / cancel 主链
- 本地 trace / dashboard / compression 日志
- Feishu 与 Weixin channel 接入
- 内建工具：Shell / Read / Write / Edit / ApplyPatch / Glob / Grep / CodeSearch / WebSearch / TapeInfo / TapeHandoff

## 仓库结构

```text
apps/
  agent-server/   axum server、SSE、CLI self、channel bridge
  web/            React + Vite+ 主工作台
crates/
  aia-config/     共享默认值、路径、稳定标识
  agent-core/     领域模型、工具协议、共享类型
  agent-prompts/  prompt 模板与工具描述
  agent-runtime/  agent loop、工具执行、事件流、压缩
  agent-store/    SQLite session / trace / provider / channel 存储
  session-tape/   append-only 会话磁带
  builtin-tools/  内建工具
  openai-adapter/ OpenAI Responses / Chat Completions 适配
  channel-bridge/ channel 抽象桥接
  channel-feishu/ Feishu transport
  channel-weixin/ Weixin transport
  weixin-client/  微信私有桥接协议 client
  provider-registry/
```

## 本地运行

### 依赖

- Rust（workspace 使用 edition 2024）
- `pnpm`

### 启动开发环境

```bash
just web-install
just dev-server   # 启动后端 http+sse server
just dev-web      # 启动前端
```

或者一起启动：

```bash
just dev
```

默认情况下：

- Web 开发服务器：`http://localhost:5173`
- `agent-server`：`http://localhost:3434`

### 运行 self 模式

```bash
cargo run -p agent-server -- self
cargo run -p agent-server -- self "继续整理 docs 和架构边界"
```

## 常用命令

```bash
just fmt
just check
just test

just web-typecheck
just web-test
just web-check
just web-build
```

## 数据落盘

默认数据目录在 `.aia/` 下：

- `.aia/store.sqlite3`：session 元信息、provider、channel、trace 等 SQLite 数据
- `.aia/sessions/*.jsonl`：每个 session 的 append-only tape

## 文档地图

- `docs/requirements.md`：项目目标、边界、非目标
- `docs/architecture.md`：共享 crate / app 壳职责与结构边界
- `docs/status.md`：**当前真实状态** 与下一步优先级
- `docs/todo.md`：未完成 backlog
- `docs/evolution-log.md`：历史演进记录
- `docs/rfc/`：设计提案与架构决策
- `docs/frontend-web-guidelines.md`：Web 前端约束

如果你想先快速了解当前仓库，建议阅读顺序是：

1. `README.md`
2. `docs/status.md`
3. `docs/requirements.md`
4. `docs/architecture.md`
5. 相关 RFC
