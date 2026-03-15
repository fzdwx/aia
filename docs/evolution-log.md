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
