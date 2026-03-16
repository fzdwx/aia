# 演进日志

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

## 2026-03-16 Session 17

**诊断**：工作区里已有一半完成的 `aia-config` 共享配置 refactor，`agent-server` / `provider-registry` / `session-tape` / `agent-runtime` 已开始引用新 crate，但 `aia-config` 自身的 `lib.rs` 仍和子模块重复定义同一批路径/标识/默认值，且这批改动还未被正式收口记录。
**决策**：先把这条已在途的高价值共享配置收口完成：让 `aia-config` 成为真正的单一配置源，并验证所有已接入 Rust crate；这是遵循“close before you open”的最小闭环。
**变更**：
- `crates/aia-config/src/lib.rs`：改为薄 façade，统一 `pub use` `paths` / `identifiers` / `server` 子模块，移除重复定义并保留集中测试。
- `crates/aia-config/src/paths.rs`、`crates/aia-config/src/identifiers.rs`、`crates/aia-config/src/server.rs`：作为共享配置的真实实现来源，承载 `.aia` 路径、默认 session/server 常量、trace/span/prompt-cache 标识与 user agent helper。
- `crates/agent-runtime/src/runtime/helpers.rs`、`crates/provider-registry/src/registry.rs`、`crates/session-tape/src/tape.rs`、`apps/agent-server/src/main.rs`、`apps/agent-server/src/model.rs`、`apps/agent-server/src/session_manager.rs`、`apps/agent-server/src/sse.rs`：继续复用 `aia-config` 的共享默认值与标识 helper，收口分散常量。
- `README.md`、`docs/status.md`、`docs/architecture.md`：同步记录 `aia-config` 已覆盖的共享配置边界。
**验证**：`cargo test -p aia-config -p agent-runtime -p provider-registry -p session-tape -p agent-server` 通过；`cargo check -p aia-config -p agent-runtime -p provider-registry -p session-tape -p agent-server` 通过。
**提交**：待提交
**下次方向**：继续收口这批共享配置接入的剩余表层影响，优先清理未提交的 Web/tooling 变更与 Rust 侧仍分散的默认值常量。

