# 需求说明

## 愿景

做一个正常、好用、性能克制、跨平台的代理运行壳，以 Web 界面为当前主承接点，并可继续被桌面壳复用。

## 核心需求

### 1. 交互形态

- 提供一个好用的 Web 界面
- 提供桌面应用支持
- 支持 Windows、Linux、macOS
- 当前以 `apps/web` + `apps/agent-server` 作为主交互承接形态
- `apps/agent-server` 除 Web 后端模式外，也应支持少量直接 CLI 驱动入口；至少要能在不打开 Web 的情况下读入 `docs/self.md` 并开始同一 runtime 主链上的自我进化对话

### 2. 运行特性

- Web 界面保持流畅、低闪烁、低阻塞
- 作为代理运行壳时注重性能
- 不能在内存和处理器占用上走极端
- 不以跑分最大化为第一目标
- 应保持 server 作为“其他客户端可驱动接口层”的能力，而不是只服务单一前端页面

### 3. 代理能力

- 感知不同模型的人格差异
- 默认内建但可选启用：工具搜索、MCP、子代理、异步子代理、分叉、代理到代理协作
- 内建常用编码工具，并且可切换启停
- 内建编码工具的对外契约应保持短名、稳定、与具体执行器解耦，避免把底层实现细节直接暴露给模型
- 兼容 Claude 与 Codex 风格的工具规范
- 支持增量压缩与交接
- 可以作为驱动其他客户端的接口层
- 取消 / stop 语义需要贯穿 server、runtime、provider streaming 与工具执行路径
- 本地 trace 诊断需要能还原 agent loop、LLM 请求与工具执行的关系
- 本地 trace 诊断还需要能单独查看上下文压缩调用与压缩摘要日志，而不只是把它们混进普通对话请求里
- `/api/traces/overview` 这类诊断接口的分页必须真正约束返回列表项数量；不能再出现“按 loop 分页但把整组 item 全展开返回”的伪分页语义。与此同时，overview 汇总应在本地存储层有可复用快照，而不是每次请求都全表重算。

## 当前阶段边界

### 已完成

- Rust 工作区骨架已建立
- 共享核心库边界已拆分为 `aia-config`、`agent-core`、`session-tape`、`agent-runtime`、`channel-registry`、`provider-registry`、`openai-adapter`、`agent-store`
- `aia-config` 已承担跨 crate 复用的应用级路径、默认值、稳定标识与构造 helper
- `channel-registry` 已承担外部 channel 静态配置与本地持久化
- `provider-registry` 已承担本地 provider 管理与持久化
- 首个真实模型适配库 `openai-adapter` 已建立，并已同时覆盖 Responses 与 OpenAI 兼容 Chat Completions 两条协议链路
- `openai-adapter` 当前已改为原生 async `reqwest`：单次请求与流式 SSE 不再依赖 blocking client
- OpenAI 请求当前已自动启用 prompt caching：server 会为同一 session 生成稳定 `prompt_cache_key`，并固定使用 `24h` retention
- `apps/agent-server` 已可编译、测试并运行，作为 Web 主界面与其他客户端的共享运行时桥接壳
- `apps/agent-server` 启动路径、路由序列化路径与本地 store 锁中毒路径都已收口为非 panic 错误路径
- 会话磁带、结构化锚点、handoff 事件、工具启停基础能力已落地
- 工具调用与工具结果已进入类型化会话磁带，并能投影到后续默认上下文
- 工具调用与工具结果现已通过稳定调用标识关联，便于后续 replay 与压缩
- 历史轮次当前由磁带 entries 按 `meta.run_id` 重建，不再把轮次块直接落盘到 `.aia/session.jsonl`
- `session-tape` 已补齐命名锚点、查询切片、命名磁带存储抽象与 fork / merge 语义
- `session-tape` TapeEntry 已改为扁平 `{id, kind, payload, meta, date}` 模型，对齐 republic 数据模型
- 旧格式 JSONL 可兼容载入并自动转换为新扁平格式
- `.aia/session.jsonl` 当前统一以扁平 `TapeEntry` JSONL 形式 append-only 落盘
- provider 本地资料当前落盘在 `.aia/providers.json`
- channel 本地资料当前落盘在 `.aia/channels.json`
- 本地 SQLite 状态当前落盘在 `.aia/store.sqlite3`
- provider 当前已具备协议级区分能力，可在同一地址 / 模型下区分 Responses 与 Chat Completions
- `apps/web` 已建立为实际主工作台，而不是仅布局骨架
- Web 客户端当前已接入 provider 管理、session 列表 / 历史 / 当前轮次恢复、流式消息展示、trace 诊断视图
- Web 客户端当前也已接入飞书 channel 管理：列表、创建、编辑、删除与启停配置
- 内建基础编码工具名已收口为 `shell`、`read`、`write`、`edit`、`glob`、`grep`，其中 `shell` 当前以内嵌 `brush` 库执行
- `builtin-tools` 的 `shell` 已把输出聚合、abort 轮询与输出捕获改为 async 事件泵，长命令等待不再依赖同步 `recv_timeout` 循环，也不再依赖 `spawn_blocking` pipe reader 桥接
- `builtin-tools` 的 `read` / `write` / `edit` 已切到 `tokio::fs`，`glob` / `grep` 也已改为共享的 async `.gitignore` 感知仓库遍历 + async 文件读取，不再依赖 `spawn_blocking` / `ignore::WalkBuilder`
- 运行时事件已统一通过共享事件模型暴露，并支持多个订阅者独立消费
- 默认上下文已改为从最新锚点之后重建，而不是无条件带上全量历史
- `agent-runtime` 已从单次模型调用收敛为单轮内多步执行：模型 → 工具 → 再回模型
- 工具不可用、工具执行失败、工具结果错配已改为轮次内结构化失败结果，而不是直接终止整个会话循环
- Web 流式 turn 已与共享运行时失败语义对齐：当前轮失败会通过 SSE 发出错误事件，但不会直接结束整个交互会话
- cached prompt usage 已贯通到 `completion.usage`、trace 存储、trace 汇总与 Web 聊天/诊断展示
- `apps/agent-server` 当前由后台 runtime worker 独占运行时，provider / history / current-turn 读取走共享快照
- `apps/agent-server` 当前已补齐 channel 控制面；飞书 channel 的目标接入模式明确为长连接，当前仓库里仍保留 webhook 过渡入口以维持主链可验证，但后续应收敛到单一长连接接入形态
- `agent-store` 当前已承担外部 conversation → `session_id` 映射与 channel message receipt 幂等去重
- `apps/agent-server` 的 turn 执行已去掉 `tokio::spawn_blocking`，session manager 与 turn worker 当前都由原生 Tokio async task 承载，不再依赖独立 current-thread Tokio worker thread
- `apps/agent-server` 的 trace 查询路由已去掉 per-request `spawn_blocking` 包装，当前直接复用共享 SQLite store 读取路径
- provider 变更已采用事务式提交：候选 registry 校验、registry 落盘、session tape 落盘全部成功后才更新内存 runtime / tape
- 已完成完整 stop/cancel 基线，并继续打通到 OpenAI streaming 与 embedded shell `TERM` 中断
- 本地 trace 当前已形成 OTel-shaped 诊断模型：agent loop root span、LLM client spans、tool internal spans 与本地 event timeline
- trace 列表读取已避免为每条记录反序列化完整 `provider_request` 大 JSON，优先依赖轻量 `request_summary` 里的用户消息预览
- 手动 / 空闲上下文压缩调用现在也会产生日志化 trace，并以独立 compression 日志视图暴露，而不是混入普通 trace 列表
- trace 首屏加载还需要避免同页重复请求和单连接串行放大：同一视图的摘要与分页应尽量合并成单次读取，SQLite 热查询需要有与过滤/分组形状匹配的索引

### 当前不做

- 桌面壳实现
- 完整 MCP 接入
- 多提供商真实适配扩展
- 异步子代理调度
- 完整 OTLP exporter / collector 集成

### 下一阶段优先事项

- 继续补强 stop/cancel 在不同 provider 与复杂 shell pipeline 下的实际覆盖率
- 继续补强飞书 channel 的生产级接入细节，优先把当前过渡 webhook 入口收敛为正式长连接模式，并补齐更细的群聊权限策略
- 继续把 runtime 驱动辅助从 `apps/agent-server` 上移到共享层
- 在工具协议边界进一步收稳后，推进统一工具规范向外部协议映射与 MCP 接入
- 在现有 Web / server 主路径稳定的前提下，继续补强 trace 数据模型与桌面壳复用基础
