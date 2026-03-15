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
