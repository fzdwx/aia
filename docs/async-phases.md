# 异步化阶段设计

## 目标

把 `aia` 从“server 用 `spawn_blocking` 包裹同步 runtime”的执行模型，逐步推进到“核心主链原生 async、provider/tool 执行原生 async、server 不再依赖 blocking turn wrapper”的架构，同时保持：

- 每个阶段都可编译、可验证
- 取消 / stop 语义不回退
- 不把工作区长期留在半完成状态
- crate 边界仍清晰：核心抽象在共享层，app 壳只做桥接

## 非目标

本设计文档不要求一口气完成：

- MCP
- 子代理 / async 子代理
- OTLP exporter
- 新 provider 扩展
- 前端协议大改

这些能力应建立在 async 主链稳定之后再继续推进。

## 背景问题

当前 `apps/agent-server` 的 turn 执行虽然已经通过 `spawn_blocking` 避免卡住 HTTP 层，但根因仍是共享主链大部分接口是同步风格：

- `LanguageModel` 是同步 trait
- `ToolExecutor` / `Tool` 是同步 trait
- `agent-runtime` 的 turn/tool/compression 主链是同步串行调用
- provider 适配和工具实现虽然内部已经有一些取消能力，但外层仍挂在同步接口上

这导致：

- server 运行中需要把 runtime `take()` 出去放进 blocking 任务
- 运行态 live stats 很难直接被路由读取
- async provider / async tool 的引入路径不自然
- stop/cancel 的传播仍要绕过同步边界

## 分阶段路线

### Phase 1：Async Trait 边界

**目标**：先把共享抽象改成 async，但保留同步包装入口，避免一次性改掉整个 server 执行模型。

**范围**：

- `agent-core`
  - `LanguageModel` → async trait
  - `ToolExecutor` / `Tool` → async trait
  - `ToolRegistry` 支持 async call
- `agent-runtime`
  - turn/tool/compress 主链增加 async 版本
  - 保留同步包装入口（例如在现有 tokio runtime 内 `block_on`）
- `openai-adapter`
  - 先接入新的 async trait
  - 内部实现暂时仍可保持现有 blocking reqwest
- `builtin-tools`
  - 接入 async `Tool` trait
  - 内部实现暂时仍可保持现有同步/线程模型
- `apps/agent-server`
  - `ServerModel` 接入 async trait
  - session manager 仍可暂时保持当前结构

**阶段完成标准**：

- `cargo check` 全仓通过
- 生产代码链路全部接到 async trait
- 同步包装入口仍可让现有 server 行为保持可用
- 测试全部迁移完成（mock model/tool 改成 async trait 用法）

**收益**：

- 为后续真正 async provider/tool/runtime 做好稳定抽象面
- 把重构风险先集中在 trait 边界，而不是一次改所有执行模型

**风险**：

- 需要同步迁移大量 mock / test impl
- 如果中途停下，容易出现“生产代码能编、测试全挂”的过渡态

---

### Phase 2：Provider 原生 Async

**目标**：把 provider 适配从“实现 async trait，但内部还是 blocking I/O”推进到真正 async HTTP/streaming。

**范围**：

- `openai-adapter`
  - 从 `reqwest::blocking` 切到 async `reqwest`
  - 流式 SSE 读取改成真正 async stream
  - 保留当前 stop/cancel 语义
- `apps/agent-server`
  - `ServerModel` trace 记录路径适配 async provider 调用

**阶段完成标准**：

- OpenAI Responses / Chat Completions 的 async 调用与 streaming 测试通过
- 取消在 async provider 链路下不回退
- `cargo check` / 相关 crate tests 通过

**收益**：

- 去掉 provider 层最重的 blocking I/O
- 为 server 去除 `spawn_blocking` 奠定前提

**风险**：

- 需要重写流式读取与错误映射
- tracing / request timeout / cancel 语义容易在切换时回退

---

### Phase 3：Tool 原生 Async

**目标**：把工具系统从“async trait + 同步实现”推进到真正可异步执行，尤其是 shell / 长任务取消语义。

**范围**：

- `builtin-tools`
  - 文件工具可继续同步包装，但接口保持 async
  - `shell` 重点收口为更适合 async runtime 的执行模型
- `agent-runtime`
  - tool call path 全面基于 async tool executor
  - runtime tool bridge 保持边界清晰

**阶段完成标准**：

- tool call 主链全面 async
- shell cancel 不回退
- 相关 runtime/tool 测试通过

**收益**：

- runtime 不再需要围绕同步工具做额外绕路
- 长工具调用与取消语义更自然

**风险**：

- shell 是最复杂点，容易引入平台差异和取消边界问题

---

### Phase 4：Server 原生 Async Turn Loop

**目标**：最终去掉 `apps/agent-server` 在 turn 执行上的 `spawn_blocking` 包装，让 session manager 直接运行 async runtime turn。

**范围**：

- `apps/agent-server`
  - `handle_submit_turn` 从 blocking worker 切到原生 async task
  - 运行态 `session/info` 可直接读取 live runtime stats 或运行态快照
  - 重新梳理 running turn handle、runtime ownership、return path

**阶段完成标准**：

- turn 执行不再依赖 `spawn_blocking`
- 运行态 `session/info` 不再退化为全 `null`
- session 独占 / cancel / SSE 行为保持正确
- `cargo test` / `cargo check` 通过

**收益**：

- server 真正原生 async
- 运行态观测更自然
- 为后续外部 client 驱动与并发能力打更稳的基础

**风险**：

- session runtime ownership 设计最容易出错
- 如果边界没收好，可能重新引入 tape / snapshot 分叉或并发访问问题

## 建议验证策略

每个阶段都遵循：

1. 先 `cargo check` 当前受影响 crate
2. 再跑最窄的 crate tests
3. 最后跑 workspace `cargo check`
4. 文档同步：
   - `docs/status.md`
   - `docs/architecture.md`
   - `docs/evolution-log.md`
5. 验证通过后提交，不把阶段性可用成果留在工作树里漂移

## 当前状态

当前仓库已完成 **Phase 1** 与 **Phase 2**：

- `agent-core` / `agent-runtime` / `builtin-tools` / `openai-adapter` / `apps/agent-server` 已全部接到 async trait 边界
- 相关 mock / 测试实现已完成迁移，当前 `cargo check` 与受影响 crate `cargo test` 已通过
- `openai-adapter` 已从 `reqwest::blocking` 切到 async `reqwest`
- Responses / Chat Completions 的 SSE 读取已改为 async chunk streaming，并继续保留 abort 轮询与取消语义

因此，下一步最高优先级变为：

1. 进入 Phase 3，继续把工具执行主链收口为真正原生 async
2. 为 Phase 4 做准备，评估如何去掉 `apps/agent-server` turn 执行上的 `spawn_blocking`
3. 在 async 主链与工具边界进一步稳定后，再优先推进统一工具协议映射与 MCP 接入，而不是继续堆厚客户端界面
