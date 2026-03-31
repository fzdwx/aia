# 生成式 UI（Generative UI）在 aia 中的落地设想

> 本文是生成式 UI 的研究/设计文章，不是当前已实施功能说明。当前项目真源请优先看 `docs/status.md`、`docs/requirements.md`、`docs/architecture.md`。

- Last verified: `2026-03-30`

## 为什么现在写这篇文章

目前 `aia` 已经具备几条生成式 UI 的关键前置条件：

- 运行时事件已经结构化：`StreamEvent`、`TurnBlock`、`ToolInvocationLifecycle`
- 服务端已经通过 HTTP + SSE 把运行时状态稳定推送到 Web
- 工具调用、工具结果、trace span 都已经是类型化数据，而不是纯文本日志
- Web 已经能消费真实 tool span / turn block，而不是只能做字符串拼接

这意味着：

> `aia` 离“生成式 UI”真正缺的，不再是一个概念，而是一套**稳定的数据契约 + 渲染协议 + 安全边界**。

所以这篇文章不讨论大而空的愿景，而是回答一个更实际的问题：

**如果要在 `aia` 里支持 generative UI，最小可落地方案应该长什么样？**

---

## 先定义：什么是 generative UI

这里的 generative UI，不是“模型输出一段 HTML，然后前端直接 innerHTML 渲染”。

在 `aia` 语境里，更合理的定义是：

> 模型或运行时输出**受约束的结构化 UI 描述**，前端把它解释为一组安全、可版本化、可回放的 widget。

也就是说，模型生成的不是任意代码，而是：

- 某个 widget 类型
- 该 widget 的 props / data
- 它属于哪一轮 turn、哪一个消息块、哪一个工具调用
- 是否是中间态、完成态、失败态
- 可否交互、交互后回到哪个工具或哪个 action

这和现在 `aia` 已有的 tool / trace / turn block 体系是一致的：

- **模型负责“意图”和“数据”**
- **运行时负责“约束”和“编排”**
- **前端负责“解释”和“渲染”**

而不是让模型直接控制 DOM。

---

## 为什么 `aia` 适合做这件事

很多 agent 项目做 generative UI 时会遇到两个问题：

1. UI 只是文本聊天的附属产物，没有稳定事件边界
2. UI 状态不可重建，刷新页面后就丢了

`aia` 恰好在这两点上已经有基础：

### 1. append-only tape 很适合承载 UI 事实

`session-tape` 的模型天然适合记录：

- 某轮里生成了哪个 widget
- widget 的数据在什么时候更新过
- 某个 widget 最终完成还是失败
- handoff / anchor 之后哪些 widget 还应继续可见

如果未来把 widget 也作为事实写入 tape，那么页面刷新、会话恢复、trace 回放都会更自然。

### 2. runtime 已经有块级事件，而不是只有纯文本

当前 `TurnBlock` 已经区分：

- thinking
- assistant text
- tool invocation
- failure

这说明 `aia` 的主链并不是“只有一条字符串流”，而是“按语义分块的执行时间线”。

生成式 UI 最自然的落点，就是在这个块级时间线里增加一种新的块：

- `Widget`
- 或更保守一点，先让 `Assistant` 块里能带一个 `ui_payload`

### 3. Web 端已经在消费结构化状态

现在前端已经能展示：

- current turn snapshot
- tool 调用生命周期
- trace timeline
- usage / cached tokens

所以 generative UI 不需要从零开始搭一个“第二套渲染系统”，而更像是在现有消息/块渲染层上加一个新分支。

---

## 不该怎么做

在开始设计前，先把几条明显错误的方向排除掉。

### 反模式 1：让模型直接返回 HTML / JSX / TSX

不建议。原因：

- 安全边界差：容易把脚本、样式、事件处理一起带进来
- 不可版本化：前端改组件 API 后，历史会话里的 HTML/JSX 很难兼容
- 不可审计：服务端无法知道 UI 到底表达了什么结构
- 不可重建：后续很难把这些片段映射回 trace / tape / widget schema

### 反模式 2：把 generative UI 做成前端专属魔法

如果只有前端 store 临时拼 widget，而 runtime / tape / server 都不知道它存在，那么：

- 页面刷新后状态难恢复
- trace 无法解释“为什么出现这个 widget”
- 其他客户端无法复用同一协议

这会违背 `aia` 的库优先、共享运行时优先原则。

### 反模式 3：一开始就做“模型生成任意交互应用”

这太大，也不稳。

对 `aia` 来说，更合适的路径是：

1. 先支持少量受控 widget
2. 先支持单向展示
3. 再支持有限交互
4. 最后再考虑更复杂的 UI 流程

---

## 建议的系统分层

生成式 UI 最好拆成四层：

### 第 1 层：共享协议层（agent-core）

定义“模型/运行时可以表达什么 UI”。

建议引入一组共享类型，类似：

```rust
pub struct UiWidget {
    pub id: String,
    pub kind: UiWidgetKind,
    pub title: Option<String>,
    pub state: UiWidgetState,
    pub payload: serde_json::Value,
}

pub enum UiWidgetKind {
    Notice,
    KeyValue,
    Table,
    CodeDiff,
    Form,
    Steps,
}

pub enum UiWidgetState {
    Streaming,
    Ready,
    Failed,
}
```

关键点：

- **kind 必须是白名单**，不能让模型发任意组件名
- `payload` 可以灵活，但必须受 schema 约束
- `state` 必须显式建模，不能靠前端猜
- `id` 必须稳定，便于流式更新与回放

### 第 2 层：运行时层（agent-runtime）

负责把 widget 纳入 turn 执行语义。

运行时要解决的问题：

- widget 是模型直接产生的，还是工具结果映射出来的？
- widget 属于哪一轮、哪一个 block？
- widget 更新是否进入磁带？
- 压缩 / handoff 后 widget 如何重建？

建议第一阶段先支持两种来源：

1. **tool-driven widget**：工具结果通过 runtime 映射成 widget
2. **assistant-declared widget**：模型返回显式的结构化 widget 段

其中 tool-driven 更容易先落地，因为：

- 数据来源更可信
- schema 更容易固定
- 可以复用现有 tool lifecycle

### 第 3 层：服务端桥接层（apps/agent-server）

负责把 widget 作为 SSE / API 的一等公民发给前端。

这里应避免“偷偷塞到 message.content 里”的做法。更合适的是：

- `CurrentTurnBlock` 增加 `Widget`
- `TurnBlock` 增加 `Widget`
- SSE 中显式发送 widget delta / widget completed

如果暂时不想改太多事件模型，也可以先把 widget 收敛为：

- `Assistant` 块中的 `ui` 字段

但长期看，独立 `Widget` block 更清晰，因为它和文本、工具一样，都是可排序的执行产物。

### 第 4 层：前端渲染层（apps/web）

前端只做两件事：

1. 根据 `kind` 选择受控组件
2. 根据 `payload` 渲染数据

例如：

- `notice` → `NoticeCard`
- `table` → `DataTable`
- `steps` → `StepTimeline`
- `code_diff` → `DiffViewer`

前端**不执行模型生成代码**，只解释共享协议。

---

## 最小可落地范围：先做“工具结果卡片化”

如果现在要立刻推进，我建议不要先做“模型自由生成 widget”，而是先做下面这个更稳的版本：

## Phase A：Tool Result → Widget 映射

让部分工具结果，除了文本外，还能提供结构化展示数据。

例如：

- `glob`：输出文件列表 widget
- `grep`：输出匹配结果列表 widget
- `read`：输出代码片段 widget
- `edit`：输出 diff/patch widget
- `tape_info`：输出上下文压力卡片 widget

这一步的优点：

- 不需要模型学会新协议，runtime 自己就能产出 widget
- 失败面小，容易测试
- 可以立刻改善 UI 可读性
- 同时能检验 `TurnBlock::Widget` 这条主链是否合理

一个简单例子：

```json
{
  "id": "widget-grep-1",
  "kind": "table",
  "title": "grep results",
  "state": "ready",
  "payload": {
    "columns": ["path", "line", "content"],
    "rows": [
      ["src/main.rs", 12, "fn main()"],
      ["src/lib.rs", 34, "pub fn run()"]
    ]
  }
}
```

这比把结果塞成大段文本，更适合真实使用。

---

## 第二阶段：模型声明式 widget

在 Phase A 稳定后，再支持模型显式声明 widget。

这里不建议让模型直接自由输出 JSON，而更建议通过 **受控 segment** 或 **专用工具** 两种方式之一。

### 方案 A：新增 `CompletionSegment::Widget`

优点：

- 语义最正统，widget 是模型输出的一等公民
- 和 text / thinking / tool_use 并列

缺点：

- 要修改 `agent-core`、adapter、runtime、web 多层
- 不同 provider 的协议映射会更复杂

### 方案 B：提供一个 runtime tool，例如 `render_widget`

示例：

```json
{
  "name": "render_widget",
  "arguments": {
    "kind": "notice",
    "title": "计划",
    "payload": {
      "tone": "info",
      "content": "接下来我会先搜索，再读取，再编辑。"
    }
  }
}
```

优点：

- 复用现有工具协议，不需要 adapter 立刻理解新 segment
- 权限边界清晰，可以校验 schema
- 更容易做灰度实验

缺点：

- 从纯语义上看，widget 被伪装成了 tool
- trace 上需要区分“真实工具”和“UI 声明工具”

对 `aia` 当前阶段，我更推荐 **先用 runtime tool 试运行，再决定是否升级成原生 segment**。

---

## 和 trace / tape 的关系

这是 `aia` 和很多“聊天 UI 小把戏”的关键区别。

### 1. widget 应该是可追溯的

至少要能回答：

- 这个 widget 出现在第几轮？
- 它来自哪个 tool / 哪次模型 step？
- 它是中间态还是最终态？
- 它的数据后来有没有被更新？

所以 widget 不应只是前端的瞬时状态。

### 2. widget 是否要写入 tape？

我的建议是分两步：

#### 第一步：先不直接写入 tape，只把它当作运行时派生块

优点：

- 最小改动
- 不会立刻扩大 tape schema
- 可以先验证前后端协议是否稳定

代价：

- 刷新恢复能力有限
- 需要从 tool result 或 turn block 重新推导 widget

#### 第二步：稳定后把 widget 作为事实写入 tape

一旦 widget schema 稳定，可以考虑新增：

- `event: widget_emitted`
- 或 `kind: widget`

如果写入 tape，需要坚持一个原则：

> 写进去的是“结构化 UI 事实”，不是前端私有渲染细节。

例如可以记录：

- `widget_id`
- `kind`
- `payload`
- `source_run_id`
- `source_tool_invocation_id`

但不应写：

- CSS class
- React 组件名
- 前端布局状态

### 3. trace 里是否应该看到 widget？

应该，但不一定要单独当 span。

更合理的是：

- 在相关 LLM span / tool span 的 event timeline 里记录 `widget.emitted`
- 或者把 widget source 信息写进 otel-style attributes

这样做的好处是：

- trace 仍聚焦执行链路
- widget 作为执行产物被解释，而不是变成另一套无关诊断模型

---

## 安全边界

生成式 UI 很容易在“体验很好”和“安全崩坏”之间滑坡，所以边界要先定死。

### 必须坚持的约束

1. **不执行模型生成的前端代码**
2. **不允许模型指定任意组件名**
3. **所有 widget kind 必须走白名单**
4. **payload 必须通过 schema 校验**
5. **交互动作必须回到受控 action/tool 协议**
6. **不能让 widget 绕过正常权限体系**

举例：

- 一个 widget 可以声明“显示文件 diff”
- 但不能声明“点击按钮后直接写文件”，除非按钮动作最终触发的是受控工具调用

也就是说：

> widget 只负责表达和引导，不直接拥有越权执行能力。

---

## 与当前 Web 架构的衔接方式

以 `apps/web` 当前结构看，最自然的接入点有两个：

### 接入点 1：current turn block 渲染

当前流式中的 thinking / tool / text 都已经是 block。

如果新增：

```ts
type CurrentTurnBlock =
  | { kind: 'thinking'; ... }
  | { kind: 'tool'; ... }
  | { kind: 'text'; ... }
  | { kind: 'widget'; widget: UiWidget }
```

那么前端只需要在现有 block renderer 里加一个分支。

### 接入点 2：completed turn 渲染

`TurnLifecycle.blocks` 也应能带 widget block，这样：

- 流式中看到什么
- 完成后就还能回看什么

否则会出现“流式时有 widget，完成后消失”的语义断裂。

### 一个重要约束

**不要让 current-turn 和 completed-turn 使用两套完全不同的 widget 数据结构。**

最好共享同一个 `UiWidget` 类型，只是流式阶段 `state=streaming`，完成后 `state=ready/failed`。

---

## 推荐的迭代路线

这是我认为最适合 `aia` 当前阶段的推进顺序。

## Milestone 1：文档与协议草案

产物：

- 本文档
- `UiWidget` / `WidgetBlock` 的共享类型草案
- 受控 widget 白名单

目标：

- 先把系统边界说清楚
- 避免一上来在前端写死临时协议

## Milestone 2：tool-driven widget

产物：

- runtime 可为部分工具结果附加 widget
- server / web 能渲染 widget block
- 至少 1~2 个工具有卡片化展示

建议先从：

- `tape_info`
- `grep`
- `glob`

开始。

原因：

- 输出天然结构化
- 无副作用
- UI 收益高

## Milestone 3：widget 持久化与恢复

产物：

- widget 事实可重建
- 页面刷新后 completed turn 仍保留 widget
- trace 中可以看到 widget 来源

## Milestone 4：assistant-declared widget

产物：

- 通过 runtime tool 或原生 segment 让模型主动声明 widget
- 支持计划卡片、状态卡片、表格、步骤流等低风险交互

## Milestone 5：受控交互

产物：

- widget 可声明按钮 / 表单
- 交互动作回到 runtime action / tool 调用
- 权限、审计、trace 都保持闭环

---

## 一个务实的结论

对 `aia` 来说，生成式 UI 不应该被理解成“让模型生成页面”，而应该被理解成：

> 在现有结构化 agent runtime 之上，引入一套可审计、可回放、可演进的 UI 产物协议。

这件事之所以现在值得做，不是因为它“酷”，而是因为 `aia` 已经有：

- 结构化工具协议
- 结构化 turn block
- SSE 流式桥接
- trace / tape 可追溯链路

也就是说，地基已经比很多项目更接近正确形态。

真正需要克制的是：

- 不要直接执行模型生成代码
- 不要让前端偷偷定义第二套协议
- 不要在 schema 稳定前就把大量 UI 事实写死到 tape

---

## 建议的下一步实现目标

如果下一次要开始落代码，我建议优先做下面这件事：

**为 `agent-core` 增加最小 `UiWidget` 协议类型，并在 `apps/web` / `apps/agent-server` / `agent-runtime` 中打通一个 `tool-driven widget` 示例（优先 `tape_info` 或 `grep`）。**

这是 generative UI 在 `aia` 里最小、最稳、最能验证架构方向的一步。
