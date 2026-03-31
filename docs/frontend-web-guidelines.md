# Web 前端开发规范

本规范适用于 `apps/web`，用于约束后续 Web 界面的结构、视觉、一致性与可维护性。

> 本文是 **前端约束文档**，不是当前进度播报。当前真实状态与优先级请优先看 `docs/status.md` 与 `docs/todo.md`。

- Last verified: `2026-03-30`

## 1. 基本原则

1. Web 前端只负责界面、交互与展示，不重写代理运行时逻辑。
2. 共享运行时、会话磁带、provider 绑定、工具续调语义必须继续留在 Rust 核心层。
3. 组件优先复用，不允许在页面里反复堆砌一次性样式碎片。
4. 先建立全局设计令牌，再写页面；避免把颜色、阴影、间距散落在各个 JSX 里。
5. 所有视觉层次必须服务于信息层次，不能为了”炫”牺牲可读性。

## 2. 运行时桥接架构

```
React (`App` + Zustand stores) ──proxy──> axum server (`apps/agent-server`)
            │                                    │
     HTTP control plane                    session manager
     + global EventSource                  + AgentRuntime
            │                                    │
       `src/lib/api.ts`                   SSE / HTTP / channel bridge
```

### Web 主路径接口

当前 Web 主路径主要消费这些接口族：

| 类别 | 典型接口 | 说明 |
|------|----------|------|
| session 列表 | `GET /api/sessions`、`POST /api/sessions`、`DELETE /api/sessions/{id}` | session 列表与创建/删除 |
| session 水合 | `GET /api/session/history`、`GET /api/session/current-turn`、`GET /api/session/info` | 历史页、当前运行中 turn、上下文压力 |
| session 设置 | `GET /api/session/settings`、`PUT /api/session/settings` | session 级 provider/model/reasoning 绑定 |
| question 控制面 | `GET /api/session/question`、`PUT /api/session/question`、`DELETE /api/session/question` | pending question 拉取、回答、取消 |
| 消息与队列 | `POST /api/session/message`、`GET /api/session/queue`、`DELETE /api/session/queue/{id}`、`POST /api/session/interrupt` | Web 主路径优先走 queue-aware 消息入口，而不是直接依赖旧的 `POST /api/turn` |
| provider / channel | `GET /api/providers/list`、`/api/providers/*`、`/api/channels`、`/api/channels/catalog` | Settings 工作台使用 |
| trace | `GET /api/traces/overview`、`GET /api/traces/dashboard`、`GET /api/traces/{id}` | Trace 工作台使用 |
| 全局事件流 | `GET /api/events` | 所有 session 的 SSE 结构化事件流 |

### SSE 事件类型

- `stream`：流式内容增量（thinking / text / tool lifecycle）
- `status`：轮次状态变更（`waiting` / `waiting_for_question` / `thinking` / `working` / `generating` / `finishing` / `cancelled`）
- `current_turn_started`：运行中 turn 的首个完整快照
- `turn_completed`：完整 `TurnLifecycle` 数据
- `context_compressed`：压缩完成提示
- `sync_required`：客户端落后，需要重拉 session 状态
- `error`：结构化错误事件
- `session_created` / `session_updated` / `session_deleted`：session 列表投影事件
- `turn_cancelled` / `turn_interrupted`：取消与打断相关事件
- `message_queued` / `message_deleted` / `queue_processing`：消息队列投影事件

### 前端状态模型

```
chatState: idle | active

active session 内的细粒度 turn 状态：
waiting → thinking / working / generating → finishing
   ├─→ waiting_for_question
   └─→ cancelled
```

- `chatState` 只表达“当前 session 是否有运行中 turn”
- 更细的执行阶段由 `StreamingTurn.status` / `CurrentTurnSnapshot.status` 表达
- 当进入 `waiting_for_question` 时，输入区会切换为 `PendingQuestionComposer`，而不是继续展示普通输入框

## 3. 目录与边界

- `src/App.tsx`：页面编排与顶层布局，不承担细碎组件实现。
- `src/features/chat/`：聊天时间线、输入区、工具渲染、diff、消息分组等聊天特性代码。
- `src/features/navigation/`：侧栏、主视图切换、settings / trace 导航承接。
- `src/features/settings/`：provider / channel 配置工作台。
- `src/features/trace/`：trace overview、workspace、detail modal、inspector。
- `src/features/channels/`：channel 专题 UI（如微信登录、channel 表单辅助）。
- `src/stores/`：Zustand store 边界；聊天、settings、trace、provider、channel、pending question 分层放在这里。
- `src/components/ui/`：通过 `shadcn` 生成或维护的基础组件。
- `src/components/`：少量跨 feature 复用的低层展示组件，例如 Markdown、theme provider、通用 AI elements。
- `src/lib/api.ts`：HTTP + SSE 客户端，封装所有与后端的通信。
- `src/lib/types.ts`：TypeScript 类型定义，镜像 Rust 侧类型。
- `src/lib/`：纯工具函数、常量、视图映射辅助。
- `src/index.css`：全局设计令牌、基础排版、背景、主题变量、动画与少量通用工具类。

原则上：**feature 内的展示与状态不要回流到 `src/components/` 里变成新的杂物堆。**

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

## 7. 排版与语义类规范

1. 排版必须先定义全局 token，再在组件中消费语义类；禁止在页面或业务组件里长期散写 `text-sm`、`text-xs`、`tracking-*` 作为主路径方案。
2. 同一语义角色必须只有一个主要入口：主标题、区块标题、正文、辅助说明、元信息、代码文本都应各自落到稳定语义类，不允许同一角色在多个页面里切换不同字号体系。
3. 基础组件的标题控制权必须收在组件或共享语义层，不允许在调用处反复用 `className` 覆盖 `DialogTitle`、`CardTitle` 之类的默认排版语义；若现有变体不足，应扩展组件语义，而不是继续叠原子类。
4. 工作台类界面允许高密度，但密度优先通过布局、间距、边框和容器控制，不通过把主内容整体降成小字来换空间。
5. `caption/meta/micro/nano` 只能用于弱化信息：如补充说明、时间、协议标签、技术元数据、热力图轴标签；任何承担主判断、主导航、状态反馈、关键解释责任的文本都不得长期停留在最小字号层。
6. 状态文案必须单独看作一层语义：`loading / empty / error / no-selection / no-results` 要在同一工作台里共用统一层级，不能一处是 `caption`、另一处退回原生 `text-sm` 或脚注级小字。
7. 代码、表格、payload 和 JSON 允许比正文更紧凑，但必须继续可扫读；如果为了塞进更多信息而让用户必须逐项辨认，视为排版失败，不是“高密度”。
8. 大写标签、字距和字重只能作为层级辅助，不应替代字号语义本身；不要出现“字号相同但靠 tracking/uppercase 假装分层”的伪层级。
9. 新增页面或工作台前，先明确该页面的四级排版关系：当前对象/标题、主要内容、辅助说明、元信息/细标签；未定义这四级前不应开始堆局部 class。

## 8. 工作台排版约束

1. 侧边栏、设置页、Trace、Channels 等工作台界面应优先复用统一的导航/次级导航/分组标签语义，而不是每个区域各自发明一组字号。
2. 主导航必须稳定强于次级动作按钮；分组标签可以更弱，但不能比主导航更抢眼。
3. Overview / Dashboard 类页面必须保留“主指标值 > 指标名/单位 > 说明文”的层级，不允许把 KPI、label、delta、caption 收平到一个字号带。
4. Detail modal / inspector 类页面必须保留“页面标题 > 卡片标题 > 内容块标题 > 说明文/元信息”的顺序，不允许把所有标题压成正文级，也不允许混入多套标题 token。
5. 空状态与错误状态首先是状态反馈，不是脚注；其标题和正文要明显高于元信息层，但不应与页面主标题抢位。
6. 若某块 UI 看起来“字体大小不同步”，优先检查是否存在多个语义入口并存，而不是先去微调某一个 `px/rem` 值。

## 9. 信息架构规范

Web 主界面按以下信息层次组织：

1. 左侧边栏：session 列表，或 settings / trace 的次级导航
2. 主内容区：在 chat / settings / trace 三个工作台之间切换
3. 聊天工作台：历史 turn、流式 turn、工具时间线、压缩提示、错误与滚动恢复
4. 输入区：模型选择、思考等级、消息队列提示、发送/打断按钮
5. question 场景：存在 pending question 时，用 `PendingQuestionComposer` 直接替换普通输入区

## 10. 运行时桥接规范

1. React 不直接编排 agent loop。
2. Web 端通过全局 SSE（`EventSource`）消费 `/api/events` 的结构化事件流。
3. SSE 连接入口在 `src/lib/api.ts` 的 `connectEvents(...)`；当前由 `App.tsx` 连接，并把事件交给 `useChatStore().handleSseEvent(...)` 投影。
4. Web 主发送路径优先走 `POST /api/session/message`，让 server 统一决定“立即开始 / 入队等待”；不要在前端平行重做一套 busy 判定。
5. session 切换时的水合不只包含 `history/current-turn/info`，还会补拉 `session settings`、`pending question` 与消息队列等附属状态。
6. `useChatStore` 负责聊天主链、SSE 投影与 session 水合；`session-settings-store`、`pending-question-store`、`trace-store`、`provider-registry-store`、`channels-store`、`workbench-store` 分别承接各自边界，不要把这些状态重新糊回一个超级 store。
7. 流式 tool 输出按 `invocation_id` 分组实时渲染，不等 `turn_completed`。
8. 收到 `sync_required` 时，应优先重拉当前 session 与相关工作台数据，而不是假设本地流式状态仍然可靠。

## 11. 开发工作流

- 前端相关命令统一优先通过仓库根目录 `just` 运行，而不是在 `apps/web` 目录里各自发明入口
- `just web-install`：安装前端依赖；优先复用本地 Vite+，首次引导时回退到 `pnpm install`
- `just web-dev`：启动前端开发服务器
- `just web-build`：构建前端生产包
- `just web-preview`：预览前端生产包
- `just web-lint`：运行前端 lint
- `just web-format`：运行前端格式化
- `just web-typecheck`：运行 TypeScript 类型检查
- `just web-test`：运行前端测试
- `just web-test-watch`：以 watch 模式运行前端测试
- `just web-check`：运行前端全量检查
- `just dev`：同时启动后端与前端；当前以后端 `cargo run -p agent-server` + 前端 `just web-dev` 执行
- `just dev-server`：仅启动后端
- `just dev-web`：仅启动前端；当前等价于 `just web-dev`
- `just typecheck`：TypeScript 类型检查；当前等价于 `just web-typecheck`
- 若前端本地脚本语义后续变化，以仓库根目录 `justfile` 与 `apps/web/package.json` 的当前定义为准，不要继续沿用旧文档中的固定命令描述
- Vite 开发服务器自动代理 `/api` 请求到 `http://localhost:3434`

## 12. 代码质量要求

1. TypeScript 不允许用类型逃逸掩盖问题。
2. 页面新增交互时，优先写出可测试、可拆分的纯映射逻辑。
3. 任何影响主界面结构的变更，都必须同步更新：
   - `docs/requirements.md`
   - `docs/architecture.md`
   - `docs/status.md`
4. 任何新增或调整的前端语义类，都必须在 `src/index.css` 或等价共享入口定义清楚职责与适用层级；不允许只在单个组件里“先写着用”。
5. 出现“调用处覆盖基础组件默认排版语义”的现象时，应优先视为规范缺口并回到共享层修正，而不是继续复制同类覆盖。
