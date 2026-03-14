# Web 前端开发规范

本规范适用于 `apps/web`，用于约束后续 Web 界面的结构、视觉、一致性与可维护性。

## 1. 基本原则

1. Web 前端只负责界面、交互与展示，不重写代理运行时逻辑。
2. 共享运行时、会话磁带、provider 绑定、工具续调语义必须继续留在 Rust 核心层。
3. 组件优先复用，不允许在页面里反复堆砌一次性样式碎片。
4. 先建立全局设计令牌，再写页面；避免把颜色、阴影、间距散落在各个 JSX 里。
5. 所有视觉层次必须服务于信息层次，不能为了”炫”牺牲可读性。

## 2. 运行时桥接架构

```
React (Vite :5173)  ──proxy──>  axum server (:3434)
     │                               │
  POST /api/turn          spawn_blocking
     │                               │
  EventSource             AgentRuntime
  GET /api/events         handle_turn_streaming()
  (全局 SSE)              broadcast::channel
```

### HTTP API

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/providers` | 返回当前 provider 信息 |
| GET | `/api/session/history` | 返回已完成的 `TurnLifecycle[]` |
| GET | `/api/session/current-turn` | 返回当前运行中的 turn 快照（如果存在） |
| GET | `/api/events` | 全局 SSE 事件流（stream / status / turn_completed / error） |
| POST | `/api/turn` | 发送用户消息，202 fire-and-forget，事件通过 SSE 返回 |

### SSE 事件类型

- `stream`：流式内容增量（thinking_delta / text_delta / tool_output_delta）
- `status`：轮次状态变更（waiting / thinking / working / generating）
- `turn_completed`：完整 `TurnLifecycle` 数据
- `error`：错误信息

### 前端状态机

```
idle → (submitTurn) → active/waiting → thinking → working → generating → idle
                                  ↑         ↓         ↓
                                  └─────────┴─────────┘ (状态可交替)
```

- `waiting`：已发送，等待 AI 响应
- `thinking`：模型思考中（收到 thinking_delta）
- `working`：工具调用执行中（收到 tool_output_delta）
- `generating`：文本生成中（收到 text_delta）

## 3. 目录与边界

- `src/App.tsx`：页面编排与顶层布局，不承担细碎组件实现。
- `src/components/ui/`：通过 `shadcn` 生成或维护的基础组件。
- `src/components/`：项目级组合组件，例如会话面板、消息列表、状态指示器。
- `src/hooks/`：React hooks，如 `use-chat.ts`（全局 SSE 连接与状态管理）。
- `src/lib/api.ts`：HTTP + SSE 客户端，封装所有与后端的通信。
- `src/lib/types.ts`：TypeScript 类型定义，镜像 Rust 侧类型。
- `src/lib/`：纯工具函数、常量、视图映射辅助。
- `src/index.css`：全局设计令牌、基础排版、背景、主题变量、动画与少量通用工具类。

## 4. 组件规范

1. 基础组件优先使用 `shadcn` 生成结果作为起点。
2. 对 `shadcn` 组件的修改必须是”贴合项目语义”的，不要直接堆大量行内 class 覆盖。
3. 页面中的重复视觉块必须抽成组件，不允许复制粘贴三次以上。
4. 组件 props 要表达语义，不要用含糊开关名。
5. 默认优先无状态展示组件；只有确实需要时再引入本地状态。
6. Markdown 展示必须通过共享渲染组件统一处理，禁止在消息组件里继续手写零散解析逻辑。

## 5. 类型规范

1. TypeScript 类型定义集中在 `src/lib/types.ts`，镜像 Rust 侧结构。
2. 使用 discriminated union（`kind` / `status` 字段区分）保持与 Rust `#[serde(tag = “...”)]` 一致。
3. 流式累积状态（`StreamingTurn`）与已完成状态（`TurnLifecycle`）分开定义，不混用。
4. 前端类型命名与 Rust 侧保持一致（camelCase 除外），避免两套术语漂移。

## 6. 样式规范

1. 禁止使用随意硬编码颜色；统一收敛到 `index.css` 的变量或语义类。
2. 布局优先使用稳定栅格与弹性布局，不依赖魔法数字临时拼接。
3. 同一页面里控制 1 套主视觉语气，不要混入多个互相打架的设计风格。
4. 深色主题优先，因为当前产品基调与运行时场景更贴近深色工作台。
5. 动效要克制：只在层级切换、悬浮反馈、进入过渡、状态指示上使用。
6. 状态指示器使用 shimmer 文字效果（`shimmer-text` class），不使用独立动画元素。

## 7. 信息架构规范

Web 主界面按以下信息层次组织：

1. 左侧边栏：provider 信息（模型名、连接状态指示灯）
2. 中央消息列表：已完成的 turn（thinking / assistant / tool / failure 块）+ 流式 turn
3. 底部输入框：发送区域，streaming 时禁用
4. 流式状态：shimmer 文字显示当前阶段（Waiting / Thinking / Working / Generating）

## 8. 运行时桥接规范

1. React 不直接编排 agent loop。
2. Web 端通过全局 SSE（`EventSource`）消费 `/api/events` 的结构化事件流。
3. 消息提交通过 `POST /api/turn` fire-and-forget，响应通过 SSE 返回。
4. `useChat` hook 是唯一的事件消费入口，管理 turns / streamingTurn / chatState / error。
5. 会话恢复通过 `GET /api/session/history` + `GET /api/session/current-turn` 加载，provider 信息通过 `GET /api/providers` 加载。
6. 流式 tool 输出（`tool_output_delta`）按 `invocation_id` 分组实时渲染，不等 turn_completed。

## 9. 开发工作流

- `just dev`：同时启动后端（cargo run -p agent-server）和前端（bun dev）
- `just dev-server`：仅启动后端
- `just dev-web`：仅启动前端
- `just typecheck`：TypeScript 类型检查
- Vite 开发服务器自动代理 `/api` 请求到 `http://localhost:3434`

## 10. 代码质量要求

1. TypeScript 不允许用类型逃逸掩盖问题。
2. 页面新增交互时，优先写出可测试、可拆分的纯映射逻辑。
3. 任何影响主界面结构的变更，都必须同步更新：
   - `docs/requirements.md`
   - `docs/architecture.md`
   - `docs/status.md`
