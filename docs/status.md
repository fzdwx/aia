# 项目状态

## 当前阶段

- 阶段：Web 界面实现
- 当前步骤：移除 TUI 路线，切换到 `apps/web` 作为主界面承接点；共享运行时、会话磁带、provider 管理与双协议模型适配继续保留

## 已完成

- 建立 Rust 工作区
- 建立 `agent-core`
- 建立 `session-tape`
- 建立 `agent-runtime`
- 建立 `provider-registry`
- 建立 `openai-adapter`
- 建立 `agent-cli` 验证壳
- 建立 `apps/web` Web 工程骨架
- 完成最小可运行验证与基础测试覆盖
- 完成项目级命名从 `like` 收敛为 `aia`
- 完成本地 provider 注册、活动项持久化与协议类型区分
- 完成 OpenAI Responses 与 OpenAI 兼容 Chat Completions 双协议适配
- 完成会话磁带扁平化、锚点、handoff、fork / merge、查询切片与旧格式兼容
- 完成结构化工具调用 / 工具结果 / 模型检查点贯穿运行时主链
- 完成 Responses 的 `previous_response_id` 续调与 Chat Completions 的原生工具链路映射
- 完成运行时单轮多步模型 / 工具循环、重复工具调用防重、预算提示、文本收尾步与独立工具调用上限
- 完成 CLI 文本 loop 的失败非终止语义，当前轮失败不会直接结束整个会话
- 完成 `apps/web` 首页从模板页替换为项目主界面骨架
- 完成 `apps/web` 工作台首页重构，并接入 `shadcn` 基础组件体系（card、badge、input、textarea、separator、scroll-area）
- 完成 Web 主界面信息结构收敛：左侧边栏、中央消息列表、底部输入框，去掉发散型展示布局
- 完成 `docs/frontend-web-guidelines.md`，明确 Web 前端开发规范与运行时边界
- 完成删除 `agent-cli` 中所有 TUI 代码、模块声明与终端 UI 依赖

## 正在进行

- 让 `apps/web` 真正接入共享 driver / runtime，承接 provider 管理、会话时间线、输入发送与流式输出
- 继续推进统一工具规范向外部协议映射与 MCP 接入

## 下一步

1. 为 `apps/web` 增加与共享运行时的桥接层
2. 在 Web 界面里接入 provider 创建 / 选择与会话恢复
3. 在 Web 界面里接入流式时间线、工具块、思考块与输入发送
4. 推进 MCP 风格工具协议接入
5. 继续把共享 driver 往客户端无关边界推进

## 为什么当前先做 Web，而不是继续堆终端界面

因为共享运行时、会话模型和工具协议主链已经稳定，继续维护 TUI 只会增加重复界面成本。当前更合理的方向是让 `apps/web` 直接承接主界面，再由桌面壳复用同一 Web 前端与 Rust 核心。

## 阻塞

- 当前无硬阻塞
