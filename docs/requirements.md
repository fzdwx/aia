# 需求说明

## 愿景

做一个正常、好用、性能克制、跨平台的代理运行壳，以 Web 界面为主承接点，并可被桌面壳复用。

## 核心需求

### 1. 交互形态

- 提供一个好用的 Web 界面
- 提供桌面应用支持
- 支持 Windows、Linux、macOS

### 2. 运行特性

- Web 界面保持流畅、低闪烁、低阻塞
- 作为代理运行壳时注重性能
- 不能在内存和处理器占用上走极端
- 不以跑分最大化为第一目标

### 3. 代理能力

- 感知不同模型的人格差异
- 默认内建但可选启用：工具搜索、MCP、子代理、异步子代理、分叉、代理到代理协作
- 内建常用编码工具，并且可切换启停
- 兼容 Claude 与 Codex 风格的工具规范
- 支持增量压缩与交接
- 可以作为驱动其他客户端的接口层

## 当前阶段边界

### 已完成

- Rust 工作区骨架已建立
- 共享核心库边界已拆分为 `agent-core`、`session-tape`、`agent-runtime`
- `provider-registry` 已承担本地 provider 管理与持久化
- 首个真实模型适配库 `openai-adapter` 已建立，并已同时覆盖 Responses 与 OpenAI 兼容 Chat Completions 两条协议链路
- 最小验证入口 `agent-cli` 已可编译、测试并运行
- 会话磁带、结构化锚点、handoff 事件、工具启停基础能力已落地
- 工具调用与工具结果已进入类型化会话磁带，并能投影到后续默认上下文
- 工具调用与工具结果现已通过稳定调用标识关联，便于后续 replay 与压缩
- 默认上下文里的工具结果投影也保留调用标识，避免同名工具结果混淆
- 轮次块已落盘到 `.aia/session.jsonl`，可用于后续 resume / replay
- `session-tape` 已补齐命名锚点、查询切片、命名磁带存储抽象与 fork / merge 语义
- `session-tape` TapeEntry 已改为扁平 `{id, kind, payload, meta, date}` 模型，对齐 republic 数据模型
- 锚点已简化为 `{entry_id, name, state: Value}`，不再硬编码固定字段
- 运行时不再将 TurnRecord 写入磁带，遵循 "derivatives never replace original facts" 原则
- 旧格式 JSONL 可兼容载入并自动转换为新扁平格式
- 兼容门面仍保持 `.aia/session.jsonl` 的旧格式读写，避免当前 CLI 会话文件被隐式迁移
- `agent-cli` 已可在启动时通过终端交互创建或选择 provider
- provider 本地资料当前落盘在 `.aia/providers.json`，并通过 `.gitignore` 避免误提交
- provider 当前已具备协议级区分能力，可在同一地址 / 模型下区分 Responses 与 Chat Completions
- `agent-cli` 已可进入多轮 agent loop，并支持退出指令
- `agent-cli` 已按模块拆分，当前保留为最小验证壳与文本 loop 入口
- `apps/web` 已建立 React + Vite 基础工程，并替换掉模板首页，开始承接主界面方向
- `apps/web` 已建立 Web 工作台骨架，并接入 `shadcn` 基础组件体系，开始承接主界面方向
- 当前会话会记住上次使用的 provider 绑定，除非用户在启动阶段主动替换
- 文本 loop 与后续 Web 客户端可共用统一驱动接口，便于作为其他客户端的驱动层
- 运行时事件已统一通过单一方法取回，并支持多个订阅者独立消费
- 默认上下文已改为从最新锚点之后重建，而不是无条件带上全量历史
- 启动阶段的 provider 创建流程现已支持选择 OpenAI Responses 或 OpenAI 兼容 Chat Completions 协议
- 会话记住的 provider 绑定现已包含协议信息，避免同地址同模型的不同协议互相串用
- `agent-runtime` 已从单次模型调用收敛为单轮内多步执行：模型 → 工具 → 再回模型，直到没有更多工具调用或达到内部步数上限
- 工具不可用、工具执行失败、工具结果错配已改为轮次内结构化失败结果，而不是直接终止整个会话循环
- 文本 loop 已与共享运行时失败语义对齐：当前轮失败会显示失败信息，但不会直接结束整个交互会话
- 模型续调上下文已不再只依赖扁平文本消息；工具调用与工具结果已作为结构化会话条目贯穿核心层到适配层
- OpenAI Responses 与 OpenAI 兼容 Chat Completions 在工具续调时已能按各自协议原生形态重建请求，而不是把工具结果压平为普通文本
- OpenAI Responses 现已支持基于 `previous_response_id` 的会话续调：同轮工具输出与下一轮用户输入都可沿用上一响应链，而不是重复回放全量历史
- 运行时步数与工具调用预算已配置化：默认安全护栏保留在核心层，CLI 验证壳使用更高预算，模型同时收到剩余预算提示以便更早收尾
- 已建立 `docs/frontend-web-guidelines.md` 作为 Web 前端开发规范

### 当前不做

- 完整 Web / 运行时桥接
- 桌面壳实现
- 完整 MCP 接入
- 多提供商真实适配
- 异步子代理调度
- 跨磁带视图与锚点图内存策略

### 下一阶段优先事项

- 明确内部统一工具规范与外部协议映射
- 推进 MCP 方向的工具协议接入
- 为 Web 界面准备稳定事件流与运行时桥接，并让 provider 管理复用同一事件流
- 在保持现有会话文件兼容的前提下，逐步把运行时接到更完整的命名磁带能力
- 在 `apps/web` 中承接 provider 管理、会话时间线、输入发送与流式展示
- 在运行时语义已收稳的前提下，继续推进统一工具规范向外部协议映射与 MCP 接入
