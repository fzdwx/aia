# 演进日志

## 2026-03-17 Session 11

**诊断**：虽然 Phase 1 / 2 已完成，但 `apps/agent-server` 的 turn 执行仍依赖 `tokio::spawn_blocking`，同时 `builtin-tools::shell` 还在 async tool 调用里用同步 `recv_timeout` 等待事件，这会拖住“全部改成异步”的 Phase 3 / 4 收口。
**决策**：先同时收口这两个最高杠杆缝隙：把 `shell` 的等待循环改成 async 事件泵，并把 server turn worker 从 `spawn_blocking` 换成独立 current-thread Tokio worker thread；这样既继续推进原生 async 主链，又不强行打破当前 `?Send` 模型 / 工具边界。
**变更**：
- `crates/builtin-tools/src/shell.rs`：将 embedded `brush` 的 stdout/stderr 聚合与 abort 轮询改为 async channel + timeout 事件泵，`shell` 工具调用不再在 async 路径里同步阻塞等待事件；同步更新测试 helper 与回归测试。
- `apps/agent-server/src/session_manager.rs`：移除 turn 执行上的 `tokio::spawn_blocking`，改为为每个 turn 启动独立 current-thread Tokio worker thread 承载 async runtime turn，并在结束后通过 return channel 归还 runtime ownership；新增回归测试覆盖 bootstrap turn 的完整 worker 路径。
- `docs/status.md`、`docs/requirements.md`、`docs/architecture.md`、`docs/async-phases.md`：同步记录 Phase 3 / 4 的这轮异步化进展与剩余收口方向。
**验证**：`cargo check` 通过；`cargo test -p builtin-tools shell -- --nocapture` 通过；`cargo test -p agent-server session_manager -- --nocapture` 通过；`cargo test -p agent-server` 通过（因 localhost listener 测试需脱离沙箱执行）。
**提交**：`caf5327` `refactor: asyncify shell worker and turn execution`
**下次方向**：继续把其余可能长时间占用线程的工具路径收口为更原生的 async / cancel 模型，并进一步减少 `apps/agent-server` 在运行中 `session/info` 上对 tape 快照回退和 runtime handoff 的依赖。

## 2026-03-17 Session 10

**诊断**：全异步主链 Phase 1 已把 trait 与 runtime 主链切到 async，但 `openai-adapter` 仍停留在 `reqwest::blocking`；这既违背“全部改成异步”的目标，也会继续阻塞后续 server 去 `spawn_blocking` 的 Phase 4 收口。
**决策**：直接推进 Phase 2：把 `openai-adapter` 的 Responses / Chat Completions 双协议都切到原生 async `reqwest`，流式读取改成 async chunk streaming；同时补齐测试与文档，让 Phase 1 / 2 一次收口。
**变更**：
- `crates/openai-adapter/src/responses.rs`、`crates/openai-adapter/src/chat_completions.rs`、`crates/openai-adapter/src/streaming.rs`、`crates/openai-adapter/Cargo.toml`：移除 blocking reqwest client，改为 async HTTP / async SSE chunk streaming，并保留 abort 轮询、状态码与响应体映射。
- `crates/openai-adapter/src/tests.rs`、`apps/agent-server/src/model.rs`：调整 adapter / server 测试驱动方式，验证 async reqwest 下的真实调用、流式、取消与 trace 持久化。
- `crates/agent-core/src/tests.rs`、`crates/builtin-tools/src/read.rs`、`crates/builtin-tools/src/write.rs`、`crates/builtin-tools/src/edit.rs`、`crates/builtin-tools/src/glob.rs`、`crates/builtin-tools/src/grep.rs`、`crates/builtin-tools/src/shell.rs`、`crates/agent-runtime/src/runtime.rs`、`crates/agent-runtime/src/runtime/helpers.rs`、`crates/agent-runtime/src/runtime/turn.rs`、`crates/agent-runtime/src/runtime/tests.rs`：补齐 async trait 测试迁移，并让 runtime 的同步包装入口在无当前 Tokio handle 时也能安全 fallback。
- `docs/status.md`、`docs/architecture.md`、`docs/async-phases.md`：同步记录全异步主链 Phase 1 / 2 已完成，下一步转向工具原生 async、server 去 `spawn_blocking` 与工具协议 / MCP 优先级。
**验证**：`cargo check` 通过；`cargo test -p agent-core -p builtin-tools -p openai-adapter -p agent-runtime -p agent-server` 通过。
**提交**：`6bc5253` `refactor: switch openai adapter to async reqwest`
**下次方向**：优先推进全异步主链 Phase 3 / 4：继续收口工具执行原生 async，并评估 `apps/agent-server` 如何移除 turn 执行上的 `spawn_blocking`；在共享工具边界稳定后，再优先推进统一工具协议映射与 MCP 接入。

## 2026-03-17 Session 9

**诊断**：动态测量窗口化与锚定补偿虽然改善了部分长历史场景，但在产品优先级上，“流式阶段绝不出现额外闪动/抖动”比极端长列表下的滚动精细度更重要；这套机制也额外增加了测量、补偿和调试复杂度。
**决策**：移除动态测量窗口化与锚定补偿，回到更稳定、可预测的消息列表渲染路径；继续保留前面已经证明有价值的 session 首屏瘦身、后台补页、memo 等优化，而不让测量机制影响流式体验。
**变更**：
- `apps/web/src/components/chat-messages.tsx`：删除动态高度测量、窗口化与锚点补偿逻辑，消息列表改回直接渲染可见历史 turns。
- `apps/web/src/lib/chat-virtualization.ts`、`apps/web/src/lib/chat-virtualization.test.ts`：删除不再使用的窗口化 helper 与测试。
- `docs/status.md`、`docs/architecture.md`：同步记录该机制已移除，当前优先选择稳定渲染路径。
**验证**：`bun test`（`apps/web`）通过；`bun run typecheck`（`apps/web`）通过。
**提交**：`09e1a69` `refactor: remove measured chat virtualization`
**下次方向**：如果后续仍需处理超长历史性能，优先考虑更低风险的方案，例如只对 session 列表或历史分页策略做优化，而不是重新引入会影响流式观感的复杂测量补偿。

## 2026-03-17 Session 8

**诊断**：后台补历史虽然已经改成 idle/可取消，但 idle 策略仍硬编码在 store 里用 `setTimeout` 模拟，既不够贴近浏览器真实空闲调度，也让测试注入和后续调度策略演进不够干净。
**决策**：把空闲调度正式收口成独立 helper：浏览器里优先使用 `requestIdleCallback`，不支持时再 fallback 到 `setTimeout`；store 只依赖抽象调度接口，测试继续用可注入 scheduler。这样策略更清晰，也更方便后续升级到优先级调度。
**变更**：
- `apps/web/src/lib/idle.ts`：新增独立 idle scheduler helper，统一封装 `requestIdleCallback` / `cancelIdleCallback` 与 `setTimeout` fallback。
- `apps/web/src/lib/idle.test.ts`：新增 idle helper 测试，覆盖原生 idle 与 fallback 两条路径。
- `apps/web/src/stores/chat-store.ts`：改为依赖 idle helper，并把测试注入接口升级为 `schedule/cancel` 成对调度器。
- `apps/web/src/stores/chat-store.test.ts`：同步接入新的可控 idle scheduler。
- `docs/status.md`、`docs/architecture.md`：补充 idle 调度抽象已独立化的说明。
**验证**：`bun test`（`apps/web`）通过；`bun run typecheck`（`apps/web`）通过。
**提交**：`5a30167` `refactor: extract web idle scheduler`
**下次方向**：如果继续推进这条线，优先把“后台补几页”做成动态策略，例如根据当前 session 历史长度、最近切换频率和是否正在 streaming 来决定补页力度，而不是固定页数。

## 2026-03-17 Session 7

**诊断**：session 切换虽然已经做成“最后一个 turn 先显示、其余历史后台补”，但后台补页仍会立即发起且不可取消；这会在用户快速切会话时制造无效请求，也会让非关键历史拉取继续和滚动/streaming 抢主线程与网络。
**决策**：把后台补历史页进一步收口到 idle/可取消模型：首屏 hydrate 后，只在空闲时增量补旧页；如果用户又切走 session，则直接 abort。这样能继续降低切换后的竞争负载，又不牺牲最终历史完整性。
**变更**：
- `apps/web/src/lib/api.ts`：为 `fetchHistory` 增加可选 `AbortSignal` 支持。
- `apps/web/src/stores/chat-store.ts`：新增 idle 调度与 abort 控制，session 首屏后只在空闲时补剩余历史，并在切换/删除 session 时取消后台补页；补页合并时按 `turn_id` 去重，避免重复最后一条。
- `apps/web/src/stores/chat-store.test.ts`：新增后台补页取消回归测试，并让 idle 调度在测试中可控。
- `docs/status.md`、`docs/architecture.md`：补充 session 后台补历史改为空闲时增量补页的说明。
**验证**：`bun test`（`apps/web`）通过；`bun run typecheck`（`apps/web`）通过；`cargo check` 受工作区中现有未收口的 Rust 改动阻塞，与本次前端改动无关。
**提交**：`a285357` `perf: defer and cancel session history backfill`
**下次方向**：继续观察 idle 补页在高频切 session 与长 streaming 并发时的收益；如果还需要更稳，可以再把 idle 调度升级为真正的 `requestIdleCallback` / 优先级调度抽象，并把“首屏之后补几页”做成动态策略。

## 2026-03-17 Session 6

**诊断**：即使 session 切换已经改成“最后一个 turn 先显示、其余历史后台补”，`_sessionSnapshots` 仍然沿用近似完整历史页的结构语义，容易让前端缓存再次偷偷变重，也让“快照到底是 UI 热缓存还是历史副本”边界不够清晰。
**决策**：把 `_sessionSnapshots` 正式瘦身为最小 UI snapshot：只保留最后一个 turn 与必要的 streaming/UI 元信息；完整历史继续只由接口返回、只存在当前活跃 session 状态中。这样能长期守住内存边界，也让快照职责更明确。
**变更**：
- `apps/web/src/stores/chat-store.ts`：将 `SessionSnapshot` 重构为最小 UI snapshot（`latestTurn` + `streamingTurn` + UI 元信息），移除快照里的历史页字段；同步更新 hydrate、turn 完成、分页、提交与取消路径，避免再把完整历史写回快照。
- `apps/web/src/stores/chat-store.test.ts`：调整相关断言与测试数据，验证瘦身后的快照仍能支持切换、补页与取消等主路径。
- `docs/status.md`、`docs/architecture.md`：补充 session 快照瘦身说明。
**验证**：`bun test`（`apps/web`）通过；`bun run typecheck`（`apps/web`）通过；`cargo check` 受仓库内现有 `agent-core` 未解析的 `ToolOutputSink` / `tokio` 编译错误阻塞，与本次前端改动无关。
**提交**：`048ec42` `refactor: shrink web session snapshots`
**下次方向**：如果继续沿这个方向走，优先把后台补历史页进一步改成 idle/可取消增量任务；同时可以考虑把 session 快照里的 `latestTurn` 也进一步退化成更轻的 message preview，进一步压低缓存体积。

## 2026-03-17 Session 5

**诊断**：session 切换时的延迟主要不在网络，而在切换前同步保存旧快照和切入新会话时一次性处理整页历史；即便接口本身不慢，主线程也会先被大数组复制和状态收口卡住一小段时间。
**决策**：把 session 切换首屏改成“两阶段 hydrate”：旧 session 只同步保存最后一个 turn，新 session 首先只请求/展示最后一个 turn，再后台补齐初始历史页；这样先让用户“进会话”，再继续补历史，减少切换体感延迟。
**变更**：
- `apps/web/src/stores/chat-store.ts`：切换时旧 session snapshot 改为仅保留最后一个 turn；新 session 先并发请求 `info/current-turn/latest history`，首屏只展示最新一条历史，随后后台补齐初始历史页。
- `apps/web/src/stores/chat-store.test.ts`：新增回归测试，验证旧 session 只保存最后一个 turn，以及新 session 会先以最新一条历史完成首屏 hydrate，再异步补齐更多历史。
- `docs/status.md`、`docs/architecture.md`：补充 session 切换首屏两阶段 hydrate 说明。
**验证**：`bun test`（`apps/web`）通过；`bun run typecheck`（`apps/web`）通过；`cargo check` 通过。
**提交**：`e517fa6` `perf: speed up session switch hydration`
**下次方向**：继续观察新 session “首条历史 + 后台补页”在超长会话下的感知效果；必要时再把“后台补页”进一步改成 idle/可取消的增量补齐，减少和滚动/streaming 的竞争。

## 2026-03-17 Session 4

**诊断**：动态测量窗口化已经比固定高度估算稳定，但在超长工具输出、折叠 details 或 Markdown 高度突变时，已测高度更新仍可能让当前视口突然上跳/下跳，尤其当用户正在阅读中段内容时更明显。
**决策**：继续把窗口化体验收口到“可读性优先”：抽出独立的 virtualization helper，并在高度更新时引入“首个可见 turn 锚定补偿”；这样在局部块展开/收起时，视口会尽量围绕当前阅读锚点稳定，而不是跟着总高度变化一起漂移。
**变更**：
- `apps/web/src/lib/chat-virtualization.ts`：新增动态窗口计算与锚定滚动补偿 helper，收口窗口化核心算法。
- `apps/web/src/lib/chat-virtualization.test.ts`：新增 measured window / anchor scroll 补偿测试。
- `apps/web/src/components/chat-messages.tsx`：为每个可见 turn 增加高度测量包装，并在测量值变更时记录当前首个可见 turn 的屏幕偏移；布局更新后按锚点补偿 `scrollTop`，减少超长工具输出展开/收起造成的视口跳动。
- `docs/status.md`、`docs/architecture.md`：补充聊天列表第三轮锚定稳定性收口说明。
**验证**：`bun test`（`apps/web`）通过；`bun run typecheck`（`apps/web`）通过；`cargo check` 通过。
**提交**：`787559d` `perf: stabilize chat viewport anchoring`
**下次方向**：继续观察工具输出大段折叠/展开与 streaming 同时发生时的边界行为；必要时再把锚点从“首个可见 turn”细化到“首个可见 block / DOM rect 片段”。

## 2026-03-17 Session 3

**诊断**：上一轮的轻量窗口化已经减轻了长历史重渲染，但仍依赖固定高度估算，遇到工具输出/Markdown 高度波动较大时 spacer 误差会累积；同时用户希望切换 session 时直接回到最新消息，而不是恢复旧的中段滚动位置。
**决策**：继续优化消息列表主路径：把窗口化升级为动态高度测量版，并把 session 切换滚动策略明确改成“切换即到底部、同 session 分页仍保留当前位置”；这样更符合聊天产品直觉，也能减少估算版窗口化的跳动误差。
**变更**：
- `apps/web/src/components/chat-messages.tsx`：新增基于 `ResizeObserver` 的 turn 高度测量与动态窗口化计算，减少长历史中高波动消息的 spacer 误差；session 切换时统一滚到最新消息底部，不再恢复旧 session 的中段 scrollTop。
- `docs/status.md`、`docs/architecture.md`：同步记录聊天列表第二轮滚动/窗口化收口。
**验证**：`bun test`（`apps/web`）通过；`bun run typecheck`（`apps/web`）通过；`cargo check` 通过。
**提交**：`19bab40` `perf: improve measured chat virtualization`
**下次方向**：继续观察动态测量窗口化在极长工具输出、折叠/展开 details、Markdown 图片等高度频繁变化场景下的稳定性；必要时再把“测量缓存 + 锚定可视首项”的算法继续做细。

## 2026-03-17 Session 2

**诊断**：session 快照缓存能显著减少切换闪烁，但如果无限保留所有访问过的 session，会把消息历史长期常驻内存；同时聊天列表在长历史场景下仍会整体重渲染，切换/分页时滚动位置也过于激进，经常被拖回底部。
**决策**：继续沿着前端流畅度主线收口：把 session 快照缓存改为有上限的受控缓存，并同时落一版不引新依赖的消息列表 memo + 轻量窗口化 + 按 session 恢复滚动位置；这是兼顾性能、资源和 UX 的一组同向修复。
**变更**：
- `apps/web/src/stores/chat-store.ts`：为 session snapshot 缓存增加上限与按已知 session 裁剪，避免无界常驻内存；补齐 status/stream/error/cancel 等路径对活跃 session 快照的同步更新。
- `apps/web/src/components/chat-messages.tsx`：为历史 turn / streaming turn 增加 `memo`，引入基于估算高度的轻量窗口化渲染；切换 session 时恢复各自滚动位置，历史分页加载时不再触发自动滚到底部。
- `apps/web/src/stores/chat-store.test.ts`：补回 session 切换缓存测试，并新增历史分页前插不会丢失既有 turns 的回归测试。
- `docs/status.md`、`docs/architecture.md`：记录 Web 聊天列表首轮渲染减载与滚动策略收口。
**验证**：`bun test`（`apps/web`）通过；`bun run typecheck`（`apps/web`）通过；`cargo check` 通过。
**提交**：`2403c52` `perf: reduce web chat rerenders and scroll jumps`
**下次方向**：继续把窗口化从“估算高度版”推进到更准确的动态测量，尤其优化工具输出高度波动较大时的 spacer 误差，并评估是否需要把 session 列表本身也做分页/虚拟化。

## 2026-03-17 Session 1

**诊断**：Web 切换 session 时会先清空消息区再等待 history/current-turn 水合完成，造成明显白屏闪烁；同时 store 里 `sessionHydrating`/`turnsHydrating` 命名不一致，加载态没有真正接到 UI。
**决策**：先在前端 store 引入按 session 的本地快照缓存，并把切换态改成“保留上一帧内容 + 轻量 loading 提示”；这是改善 session 切换流畅度和 UX 的最小高杠杆改动，不需要改后端协议。
**变更**：
- `apps/web/src/stores/chat-store.ts`：新增 session snapshot 缓存、统一 `sessionHydrating` 状态，并在 hydrate / turn 完成 / cancel / 分页等路径同步维护快照，切换 session 时优先展示缓存内容而不是先清空。
- `apps/web/src/components/chat-messages.tsx`：移除切换 session 时依赖 key 的整块重挂载动画，改为稳定容器 + 顶部 loading 指示，水合期间只轻微降低内容透明度，减少布局跳动和白屏感。
- `apps/web/src/components/sidebar.tsx`：为当前正在切换的 session 增加 loading 文案并禁用重复点击/删除，降低误触与不确定感。
- `apps/web/src/stores/chat-store.test.ts`：新增 session 切换缓存显示与 snapshot 更新回归测试。
- `docs/status.md`、`docs/architecture.md`：补充 Web session 切换流畅度收口说明。
**验证**：`bun test`（`apps/web`）通过；`bun run typecheck`（`apps/web`）通过；`cargo check` 通过。
**提交**：`c3ec108` `fix: smooth session switching in web chat`
**下次方向**：继续收口前端长列表渲染和滚动策略，优先评估消息区是否需要更细粒度 memo / virtualization，进一步减少超长 session 下的重渲染抖动。

## 2026-03-15 Session 1

**诊断**：`builtin-tools` 的 `read`/`write`/`edit` 关键文件工具缺少边界测试，二进制文件、较大文件窗口读取和唯一替换失败等路径的可靠性验证不足。
**决策**：先补齐文件工具的高价值边界测试，并顺手把 `read` 的二进制文件报错信息收口得更明确；这是 Tier 1 中投入最小、能直接提升 agent 编码可靠性的改进。
**变更**：
- `crates/builtin-tools/src/read.rs`：为大文件窗口读取、非 UTF-8 文件、权限拒绝场景补充测试，并将非 UTF-8 读取错误改为更明确的文本文件报错。
- `crates/builtin-tools/src/write.rs`：为自动创建父目录和大内容写入不截断补充测试。
- `crates/builtin-tools/src/edit.rs`：为多行唯一替换成功和非唯一匹配失败且不改写原文件补充测试。
- `apps/agent-server/src/main.rs`：移除未使用的 `SessionTape` 导入，消除 `cargo check` 警告。
**验证**：`cargo check` 通过；`cargo test` 通过；为 builtin file tools 新增 7 个边界测试。
**下次方向**：优先继续补 `glob`/`grep` 的边界测试，或开始把工具超时/取消机制从 shell 扩展到统一 runtime 工具执行层。

## 2026-03-15 Session 2

**诊断**：`glob`/`grep` 仍缺少对 `.gitignore`、常见忽略目录、glob 过滤、二进制文件跳过和截断元数据的回归测试，工具搜索行为在真实仓库中缺乏保护。
**决策**：继续完成 builtin 搜索工具的边界测试，并顺手修正 `grep` 对 `glob` 参数的过滤实现，使其真正只过滤候选文件而不意外关闭 ignore 语义。
**变更**：
- `crates/builtin-tools/src/glob.rs`：新增 `.gitignore` / `node_modules` / `target` 忽略与 limit 截断行为测试。
- `crates/builtin-tools/src/grep.rs`：新增 `.gitignore`、glob 过滤、二进制文件跳过与截断元数据测试，并将 `glob` 参数实现改为显式 matcher 过滤，避免 override 路径破坏 ignore 预期。
- `docs/evolution-log.md`：追加本次演进记录。
**验证**：`cargo check` 通过；`cargo test` 通过；为 `glob`/`grep` 新增 6 个测试。
**下次方向**：优先评估统一工具取消/超时机制，尤其把 shell 的 abort 能力继续上推到 runtime 工具执行层。

## 2026-03-15 Session 3

**诊断**：runtime 现在用“上一轮真实 input token”驱动上下文压力判断，但在 handoff/压缩锚点已经显著缩短上下文后，`context_stats` 和预压缩判断仍可能沿用旧 token 数，导致重复压缩或误报高压力。
**决策**：先修正 `agent-runtime` 的上下文压力估算口径，让它在保留真实 token 数据优先级的同时，能在锚点截断后回退到当前请求视角的估算值；这是提升长对话压缩可靠性的最小高杠杆修复。
**变更**：
- `crates/agent-runtime/src/runtime/request.rs`：抽出默认请求构造片段，新增当前上下文单位估算逻辑，并在检测到最近 `turn_completed` 之后已有锚点时不再盲目沿用旧 `last_input_tokens`。
- `crates/agent-runtime/src/runtime/tests.rs`：补充 `context_stats` 当前请求口径、锚点后不沿用旧 token 统计、锚点后不会误触发重复压缩等回归测试，并同步调整现有压缩测试数据以匹配新的真实口径。
- `docs/evolution-log.md`：追加本次演进记录。
**验证**：`cargo check` 通过；`cargo test` 通过；为 runtime 上下文压力逻辑新增 3 个回归测试。
**提交**：`21eede2` `docs: record context pressure fallback update`（配套代码提交：`3b7b5a6 fix: refine context pressure fallback after anchors`）
**下次方向**：继续评估统一工具取消/超时机制，或把当前“请求估算值 vs 上次真实 usage”口径进一步整理为更显式的上下文统计字段，避免前端/工具侧误读。

## 2026-03-15 Session 4

**诊断**：runtime tool bridge 仍依赖裸指针 + `unsafe` 回调访问 `AgentRuntime`，既违背项目的 `unsafe_code = "forbid"` 目标，也让 `tape_handoff` 这类 runtime tool 的执行时序更难推理。
**决策**：先把 runtime tool bridge 改成无 `unsafe` 的快照 + 延迟提交模型；这能立刻消除核心运行时里的不安全代码，并让 runtime tool 对会话的写操作边界更清晰。
**变更**：
- `crates/agent-runtime/src/runtime/tape_tools.rs`：移除裸指针桥接，改为保存 `context_stats` 快照并在 bridge 内缓存待提交的 handoff 请求，不再使用 `unsafe`。
- `crates/agent-runtime/src/runtime/tool_calls.rs`：runtime tool 执行后统一 drain 并提交 handoff 请求，保持 tool result 与后续锚点写入链路可控。
- `crates/agent-runtime/src/runtime/tests.rs`：新增 runtime tool bridge 回归测试，验证 handoff 后续请求会正确过滤孤立 `tool_result`。
- `docs/self.md`：强化自进化 prompt 的主动性、未提交改动收口纪律、硬约束优先级与“验证后必须提交”的要求。
- `docs/evolution-log.md`：追加本次演进记录。
**验证**：`cargo check` 通过；`cargo test` 通过；为 runtime tool bridge 新增 1 个回归测试。
**提交**：`760d2bb` `refactor: remove unsafe runtime tool bridge`
**下次方向**：优先继续把统一取消/中断机制上推到 runtime 工具执行层，或补一轮 workspace lint 配置收口，确保 `unsafe_code`/clippy 约束被各 crate 真正继承。

## 2026-03-16 Session 2

**诊断**：`apps/agent-server` 的 session manager 在运行态路径里对多个 `RwLock` 使用 `expect("lock poisoned")`，一旦后台任务曾在持锁期间 panic，后续读取 history/current turn 或写入 provider/session 快照时会把中毒继续升级成 server panic。
**决策**：先把 session manager 的锁访问改为显式恢复 poisoned lock，并补回归测试；这是比继续做能力扩展更高优先级的可靠性修复，而且改动局部、验证直接。
**变更**：
- `apps/agent-server/src/session_manager.rs`：新增统一的 poisoned `RwLock` 读写恢复辅助函数，替换运行态路径中的 `expect("lock poisoned")`；让 current turn/history/provider snapshot 更新在锁中毒后仍能继续服务。
- `apps/agent-server/src/session_manager.rs`：新增 3 个回归测试，验证 read/write helper 以及 `update_current_turn_status` 都能在 poisoned lock 下恢复工作。
- `docs/evolution-log.md`：追加本次演进记录并补充提交信息。
**验证**：`cargo check` 通过；`cargo test` 通过；新增 3 个 poisoned-lock 回归测试。
**提交**：`5b19d5a` `fix: recover poisoned session manager locks`
**下次方向**：继续清理 server/runtime 主路径中的其他 panic-on-poison 或 stop/cancel 缺口，优先把长轮次中断语义真正贯穿到 runtime/tool 执行层。

## 2026-03-16 Session 3

**诊断**：仓库里对“生成式 UI”只有 `docs/todo.md` 中的一条外链提醒，缺少结合 `aia` 当前 runtime / SSE / trace / tape 架构的本地设计说明，后续实现容易直接滑向前端私有协议或模型生成任意代码。
**决策**：先补一份项目内的 `generative-ui-article.md` 设计文章，把生成式 UI 的分层、边界、渐进落地路线和与现有架构的衔接方式说清楚；这是低风险但高杠杆的架构收口。
**变更**：
- `docs/generative-ui-article.md`：新增生成式 UI 设计文章，定义 `aia` 语境下的 generative UI、推荐的协议分层、安全边界、与 trace/tape 的关系，以及从 tool-driven widget 到 assistant-declared widget 的迭代路线。
- `docs/todo.md`：将原先单一外链替换为“本地设计文章 + 外部参考”，让后续实现有仓库内可追溯起点。
- `docs/evolution-log.md`：追加本次演进记录并补充提交信息。
**验证**：`cargo check` 通过；`cargo test` 通过；文档文件已纳入工作区。
**提交**：`263f220` `docs: add generative ui design article`
**下次方向**：按文档中的最小路线，优先在共享层引入最小 `UiWidget` 协议草案，并从 `tape_info`/`grep` 这类结构化结果开始试做 tool-driven widget。

## 2026-03-16 Session 4

**诊断**：stop/cancel 语义仍停留在产品需求层，运行中 turn 一旦进入长工具调用就无法从 server/Web 主路径发起中断，导致长轮次恢复与资源回收不可靠。
**决策**：先做一条最小但闭环的 cancel 主链：server 新增取消 API，session manager 持有运行中 `TurnControl`，runtime 把 abort signal 传到工具执行上下文，Web 提供 stop 按钮并显示 cancelled 状态；这是当前最有杠杆的可靠性补口。
**变更**：
- `crates/agent-runtime/src/runtime.rs`、`crates/agent-runtime/src/types.rs`、`crates/agent-runtime/src/runtime/turn.rs`、`crates/agent-runtime/src/runtime/tool_calls.rs`、`crates/agent-runtime/src/runtime/error.rs`：新增 `TurnControl` 与 `handle_turn_streaming_with_control`，让取消信号在 turn 级别与工具执行路径间传递，并把取消收敛为结构化失败事件而不是悬挂执行。
- `crates/agent-runtime/src/runtime/tests.rs`：新增预取消、工具执行中取消和 `TurnControl` 暴露回归测试，并补一条工具失败事件断言，覆盖新的取消/失败语义。
- `apps/agent-server/src/session_manager.rs`、`apps/agent-server/src/runtime_worker.rs`、`apps/agent-server/src/sse.rs`、`apps/agent-server/src/routes.rs`、`apps/agent-server/src/main.rs`：为运行中 session 保存 cancel handle，新增 `POST /api/turn/cancel`、`turn_cancelled` SSE 事件与 `cancelled` 状态，确保取消请求能落到 worker 并反馈给前端。
- `apps/web/src/lib/api.ts`、`apps/web/src/lib/types.ts`、`apps/web/src/stores/chat-store.ts`、`apps/web/src/components/chat-input.tsx`、`apps/web/src/components/chat-messages.tsx`：新增取消 API 调用、SSE 事件类型与 store 处理，输入区发送按钮在运行中切换为 stop，消息区能显示 cancelled 状态。
- `docs/status.md`、`docs/architecture.md`：更新当前 stop/cancel 进展与剩余缺口说明。
**验证**：`cargo check` 通过；`cargo test` 通过；新增 runtime/server 取消相关回归测试。
**提交**：`da2ff68` `feat: add turn cancellation flow`
**下次方向**：继续把 cancel 从当前 server→runtime→tool context 基线扩展到真正中断 OpenAI streaming 请求与 embedded shell 长任务，并评估是否把取消状态也收口进共享 turn protocol，避免前后端分别猜测。

## 2026-03-16 Session 5

**诊断**：`chore: clean` 之后仓库把 `docs/generative-ui-article.md` 删掉了，但 `docs/todo.md` 和演进历史仍把它当作本地设计基线，导致文档引用悬空、架构记录与实际仓库状态不一致。
**决策**：先恢复被误删的生成式 UI 设计文章，并校正 `docs/evolution-log.md` 里的提交记录与 session 顺序；这是对现有未收口文档改动的低风险收口，能立刻恢复设计文档可追溯性。
**变更**：
- `docs/generative-ui-article.md`：从历史提交恢复生成式 UI 设计文章，重新补回本地协议分层、安全边界与迭代路线说明。
- `docs/evolution-log.md`：修正生成式 UI / cancel 两次会话的提交信息与 session 顺序，并追加本次恢复记录。
**验证**：`cargo check` 通过；`cargo test` 通过；`docs/todo.md` 的本地文档引用重新有效。
**提交**：`0606ea8` `docs: restore generative ui design article`
**下次方向**：优先把文档中的最小 `UiWidget` 协议草案落到共享类型，并从 `tape_info` 或 `grep` 开始试做 tool-driven widget。

## 2026-03-16 Session 6

**诊断**：现有 cancel 虽已打通 server→runtime→tool context，但 OpenAI 流式读取和 embedded shell 长任务仍可能继续阻塞，且前后端仍主要靠 `failure_message` 文本猜测“这是不是取消”。
**决策**：先把取消继续下推到真实流式模型读取与 shell 作业层，并新增共享 `TurnLifecycle.outcome` 字段统一表达 succeeded/failed/cancelled；这是当前 stop/cancel 路线上最小且最有杠杆的可靠性补强。
**变更**：
- `crates/agent-core/src/traits.rs`、`crates/openai-adapter/src/error.rs`、`crates/openai-adapter/src/responses.rs`、`crates/openai-adapter/src/chat_completions.rs`：为 `LanguageModel` 新增 abort-aware 流式入口与取消错误识别；OpenAI Responses / Chat Completions 在 SSE 读取循环中主动检查取消信号并返回结构化 cancelled error。
- `crates/builtin-tools/src/shell.rs`：为 embedded brush shell 增加控制通道，取消时向当前 job 发送 `TERM` 并尽快返回，不再只靠工具层轮询后等待长命令自然结束。
- `crates/agent-runtime/src/types.rs`、`crates/agent-runtime/src/runtime/turn.rs`、`crates/agent-runtime/src/runtime/finalize.rs`、`crates/agent-runtime/src/runtime/tests.rs`：新增共享 `TurnOutcome`，让 runtime 在 provider cancelled error / 本地取消路径下统一发布 `outcome = cancelled` 的轮次结果。
- `apps/agent-server/src/model.rs`、`apps/agent-server/src/runtime_worker.rs`、`apps/agent-server/src/sse.rs`、`apps/web/src/lib/types.ts`、`apps/web/src/components/chat-messages.tsx`：把取消结果继续映射到 server / Web 共享协议，历史重建和 UI 元信息改为优先消费 `outcome`，不再依赖 `failure_message` 猜测取消状态。
- `docs/status.md`、`docs/architecture.md`：更新 stop/cancel 当前覆盖范围与剩余观察点说明。
**验证**：`cargo check` 通过；`cargo test` 通过；新增 OpenAI adapter 取消与 server model 取消识别测试，并补 shell/runtime 取消回归验证。
**提交**：`dbd0828` `feat: propagate cancellation into streaming model and shell`
**下次方向**：继续验证不同 OpenAI 兼容上游与复杂 shell pipeline 的实际中断覆盖率，必要时再把“读流中断”继续下推到更底层的 HTTP 连接级取消或 provider 超时/中止控制。

## 2026-03-16 Session 7

**诊断**：server 取消 API 在收到 cancel 请求时会立即广播 `status=cancelled` 和 `turn_cancelled`，而运行中的 worker 在轮次真正结束后又会再次广播一次，导致 cancelled SSE 重复发射、客户端需要自行去重。
**决策**：把 cancelled SSE 的发射点收口到 worker 完成路径：取消请求只负责触发 abort 和更新本地快照，不再抢先广播；这样能在不改动 HTTP API 的前提下统一“取消已请求”和“轮次已确认结束”的边界。
**变更**：
- `apps/agent-server/src/session_manager.rs`：移除 `handle_cancel_turn` 中抢先发送的 cancelled SSE，仅保留 abort 触发与 current turn 快照状态更新，并同步调整测试与告警清理。
- `apps/agent-server/src/model.rs`：补上取消识别回归测试的 `#[test]` 标记，确保该路径真正被测试执行。
- `docs/status.md`、`docs/architecture.md`：补充说明 server 侧已把 cancelled SSE 发射点收口到单一路径，避免重复事件。
**验证**：`cargo check` 通过；`cargo test` 通过；`agent-server` 取消快照测试通过。
**提交**：`f963b18` `fix: dedupe cancelled SSE emission`
**下次方向**：继续观察 stop/cancel 在真实上游和复杂 shell 任务下的覆盖率；如果 provider 侧仍有阻塞窗口，再评估更底层的 transport 取消方案。

## 2026-03-16 Session 8

**诊断**：当前取消轮次虽然已能在共享 `TurnLifecycle` 中保留取消前的 thinking / assistant / tool 内容，但 Web store 在收到 cancelled error 时会直接结束当前流式轮次，导致用户更容易只看到“本轮已取消”，而不是已经生成的部分内容。
**决策**：先修正前端 store 的取消态处理：取消只改变当前轮状态，不清空已生成块；等随后收到 `turn_completed(outcome=cancelled)` 再把完整内容落入历史。这是局部、低风险且直接提升长轮次取消体验的收口。
**变更**：
- `apps/web/src/stores/chat-store.ts`：将 cancelled error 视为状态迁移而不是流式内容清空，保留取消前已生成的 blocks。
- `apps/web/src/stores/chat-store.test.ts`：新增 2 条回归测试，验证取消错误后仍展示 partial content，以及 cancelled `turn_completed` 会把保留内容正常写入历史。
- `docs/evolution-log.md`：追加本次演进记录。
**验证**：`cargo check` 通过；`cargo test` 通过；尝试执行 `apps/web` 测试脚本但当前工程未定义 `npm test`。
**提交**：`29ea22e` `fix: preserve partial content on cancelled turns`
**下次方向**：继续补前端测试命令/基线，让 Web store 回归测试能纳入标准验证链路；随后再回到 provider / shell 的真实取消覆盖率诊断。

## 2026-03-16 Session 9

**诊断**：虽然取消轮次的共享结果已经有 `TurnOutcome::Cancelled`，但块级协议仍把取消结束表示成 `{ "kind": "failure", "message": "本轮已取消" }`，导致前端只能按失败样式渲染，语义仍然混淆。
**决策**：把取消从 failure block 中彻底拆出来，新增独立 `TurnBlock::Cancelled`，并同步收口 runtime 发布、server 历史重建与 Web 渲染；这样 cancelled 不再伪装成 failed。
**变更**：
- `crates/agent-runtime/src/types.rs`、`crates/agent-runtime/src/runtime/finalize.rs`、`crates/agent-runtime/src/runtime/tests.rs`：新增 `TurnBlock::Cancelled`，runtime 在取消时发布 cancelled block 而不是 failure block，并补回归断言。
- `apps/agent-server/src/runtime_worker.rs`：历史/快照重建时把取消型 `turn_failed` 事件映射为 `TurnBlock::Cancelled`，并新增回归测试验证 tape 重建后的块语义。
- `apps/web/src/lib/types.ts`、`apps/web/src/components/chat-messages.tsx`：前端类型与渲染支持 `cancelled` block，取消消息改用中性样式而非 destructive 失败样式。
- `docs/status.md`、`docs/architecture.md`：更新取消块级语义已经独立化的说明。
**验证**：`cargo check` 通过；`cargo test` 通过；新增 runtime/server 取消块语义回归测试。
**提交**：`b4b9162` `fix: distinguish cancelled blocks from failures`
**下次方向**：继续补 Web 测试入口，把前端取消态与 cancelled block 渲染回归纳入标准验证；随后再回到 provider / shell 取消覆盖率诊断。

## 2026-03-16 Session 10

**诊断**：即使前端已能保留 cancelled 状态下的流式块内容，provider 若是在流式过程中先输出部分 thinking/text 再返回 cancelled error，runtime 仍不会把这些已流出的 partial delta 写入 tape / `TurnLifecycle`，导致最终历史只剩取消提示而缺失真实已生成内容。
**决策**：先在 runtime 内部缓存流式 thinking/text delta，并在 cancelled provider error 路径下先 flush partial 内容再记录 cancelled 结束态；这是修复“用户看见过但历史里消失”的最小闭环补口。
**变更**：
- `crates/agent-runtime/src/runtime/turn.rs`：为 turn buffers 增加流式 thinking / assistant text 缓冲，provider cancelled error 时先把 partial output 落到 tape 和 turn blocks，再进入 cancelled failure path；正常 completion 仍沿用原有 segment 持久化链路，避免重复写入。
- `crates/agent-runtime/src/runtime/tests.rs`：新增回归测试，验证“先流出 partial output，再 cancelled error”时，最终 `TurnLifecycle` 与 tape 都会保留 thinking / assistant partial 内容并附带 cancelled block。
- `docs/evolution-log.md`：追加本次演进记录。
**验证**：`cargo check` 通过；`cargo test` 通过；新增 1 条 runtime partial-output-on-cancel 回归测试。
**提交**：待提交
**下次方向**：继续补 Web 测试入口，并让前端取消态回归真正纳入标准验证链路；之后再回到 provider / shell 的真实取消覆盖率诊断。

## 2026-03-16 Session 29

**诊断**：当前 session 执行模型之所以还依赖 `spawn_blocking`，根因不是 server 壳层本身，而是 `agent-core` 到 `agent-runtime` 的模型/工具 trait 仍是同步接口；在不先改 trait 边界的情况下，server 无法稳妥切到原生 async turn loop。
**决策**：先落全异步主链的 Phase 1：把 `LanguageModel` / `ToolExecutor` / `Tool` 改成 async trait，给 `agent-runtime` 增加 async turn 主链，并让 `openai-adapter`、`builtin-tools`、`apps/agent-server` 的生产代码先全部接上新的 async trait；同时保留同步包装入口，避免一次性推倒当前 session manager。这样既开始了 async 重构，也保持当前工作树可编译。
**变更**：
- `crates/agent-core/src/traits.rs`、`crates/agent-core/src/registry.rs`、`crates/agent-core/src/lib.rs`、`crates/agent-core/Cargo.toml`：引入 `async-trait`，把模型/工具 trait 改为 async，并让 `ToolRegistry` 支持 async 调用。
- `crates/agent-runtime/src/runtime.rs`、`crates/agent-runtime/src/runtime/turn.rs`、`crates/agent-runtime/src/runtime/tool_calls.rs`、`crates/agent-runtime/src/runtime/compress.rs`、`crates/agent-runtime/src/runtime/tape_tools.rs`、`crates/agent-runtime/Cargo.toml`：新增 async turn / tool / compression 主链，同时保留同步包装入口，生产代码继续可用。
- `crates/openai-adapter/src/responses.rs`、`crates/openai-adapter/src/chat_completions.rs`、`crates/openai-adapter/Cargo.toml`：将 provider 适配实现接到新的 async `LanguageModel` trait，但内部网络实现暂仍保留现有阻塞模型，作为后续 Phase 2 的切入点。
- `crates/builtin-tools/src/read.rs`、`crates/builtin-tools/src/write.rs`、`crates/builtin-tools/src/edit.rs`、`crates/builtin-tools/src/glob.rs`、`crates/builtin-tools/src/grep.rs`、`crates/builtin-tools/src/shell.rs`、`crates/builtin-tools/Cargo.toml`：将内建工具实现接到新的 async `Tool` trait，内部逻辑暂保持原样。
- `apps/agent-server/src/model.rs`、`apps/agent-server/Cargo.toml`：将 server model 接到 async `LanguageModel` trait，保留现有 trace 记录逻辑。
- `docs/status.md`、`docs/architecture.md`：同步记录 async 主链 Phase 1 已开始、当前仍保留同步包装与测试迁移待续。
**验证**：`cargo check -p agent-core` 通过；`cargo check -p agent-runtime` 通过；`cargo check -p builtin-tools` 通过；`cargo check -p openai-adapter` 通过；`cargo check -p agent-server` 通过；`cargo check` 通过。`cargo test -p agent-runtime --no-run` 当前仍因旧测试实现未迁到 async trait 而失败。
**提交**：待提交
**下次方向**：继续完成 Phase 1 的测试迁移，把 `agent-runtime` / `agent-server` 的 mock model/tool 全部改成 async trait 宏与 `Handle::block_on` 辅助；完成后再进入 Phase 2，把 `openai-adapter` 从 blocking reqwest 切到真正 async HTTP。 
