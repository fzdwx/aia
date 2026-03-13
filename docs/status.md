# 项目状态

## 当前阶段

- 阶段：Web 界面 ↔ 运行时桥接
- 当前步骤：`apps/web` 已通过 `apps/agent-server`（axum HTTP+SSE）桥接到共享运行时，实现流式消息、状态指示与工具调用实时展示；工作区已移除 `agent-cli`，收口为 Web + server 主路径

## 已完成

- 建立 Rust 工作区
- 建立 `agent-core`
- 建立 `session-tape`
- 建立 `agent-runtime`
- 建立 `provider-registry`
- 建立 `openai-adapter`
- 建立 `apps/web` Web 工程骨架
- 完成最小可运行验证与基础测试覆盖
- 完成项目级命名从 `like` 收敛为 `aia`
- 完成本地 provider 注册、活动项持久化与协议类型区分
- 完成 OpenAI Responses 与 OpenAI 兼容 Chat Completions 双协议适配
- 完成会话磁带扁平化、锚点、handoff、fork / merge、查询切片与旧格式兼容
- 完成结构化工具调用 / 工具结果 / 模型检查点贯穿运行时主链
- 完成 Responses 的 `previous_response_id` 续调与 Chat Completions 的原生工具链路映射
- 完成运行时单轮多步模型 / 工具循环、重复工具调用防重、预算提示、文本收尾步与独立工具调用上限
- 完成 `apps/web` 首页从模板页替换为项目主界面骨架
- 完成 `apps/web` 工作台首页重构，并接入 `shadcn` 基础组件体系（card、badge、input、textarea、separator、scroll-area）
- 完成 Web 主界面信息结构收敛：左侧边栏、中央消息列表、底部输入框，去掉发散型展示布局
- 完成 `docs/frontend-web-guidelines.md`，明确 Web 前端开发规范与运行时边界
- 完成 `apps/agent-server` axum HTTP+SSE 服务器，桥接 Web 前端到共享运行时
- 完成全局 SSE 事件流架构（`GET /api/events`），基于 `broadcast::channel` 向所有客户端推送事件
- 完成 `POST /api/turn` fire-and-forget 消息提交，响应通过全局 SSE 返回
- 完成 `GET /api/providers` 与 `GET /api/session/history` 数据接口
- 完成 Rust 侧核心类型（StreamEvent、TurnLifecycle、TurnBlock 等）的 Serialize/Deserialize 支持
- 完成前端 TypeScript 类型定义镜像 Rust 侧类型（discriminated union 对齐 serde tag）
- 完成前端 `useChat` hook：全局 EventSource 连接、流式状态累积、turn 完成回收
- 完成流式轮次状态指示：waiting → thinking → working → generating，shimmer 文字动画
- 完成流式 tool_output_delta 实时渲染，按 invocation_id 分组展示，不等 turn_completed
- 完成 Vite 开发代理配置（`/api` → `http://localhost:3434`）
- 完成 justfile 开发命令（`just dev` 同时启动前后端）
- 完成移除 `apps/agent-cli` 包，并同步清理工作区与文档中的 CLI 主入口叙事
- 完成核心 Rust crates 的内部模块化收口：`provider-registry`、`agent-core`、`session-tape`、`openai-adapter`、`agent-runtime` 已从单文件主入口拆为薄 `lib.rs` + 职责模块
- 完成 `provider-registry` 与 `apps/agent-server` 之间的 provider 多模型配置接口对齐，并兼容旧单模型本地落盘格式

## 正在进行

- 继续推进统一工具规范向外部协议映射与 MCP 接入
- 收口 `apps/agent-server` 与 `apps/web` 双壳结构后的文档与边界一致性

## 下一步

1. 推进 MCP 风格工具协议接入
2. 继续把共享 driver 往客户端无关边界推进
3. 在工具协议边界进一步收稳后，再接入 Web 端 provider 创建 / 选择与会话恢复
4. 桌面壳接入

## 为什么当前先做 Web，而不是继续堆终端界面

因为共享运行时、会话模型和工具协议主链已经稳定，继续维护独立终端壳只会增加重复界面成本。当前更合理的方向是让 `apps/web` 直接承接主界面，再由桌面壳复用同一 Web 前端与 Rust 核心；而在主界面主路径已经收口后，下一优先级应回到统一工具规范的外部映射与 MCP 接入，而不是继续提前堆厚更多客户端表层能力。

## 阻塞

- 当前无硬阻塞
