---
rfc: 0006
name: widget-host-protocol
title: Widget Host Protocol
description: Defines the shared widget host protocol, capability gating split, and phased migration from tool-only HTML widgets to a first-class widget runtime.
status: Draft
date: 2026-04-05
authors:
  - aia
supersedes: []
superseded_by: null
---

# RFC 0006: Widget Host Protocol

## Summary

为 `aia` 引入一套**共享 widget host 协议**，把当前“`WidgetRenderer` 工具输出 HTML → Web 特判渲染”的实现，逐步迁移为：

1. 共享核心里有稳定 widget 协议类型
2. runtime / server / Web 统一承载同一套 widget 语义
3. Web 侧 widget 宿主能力通过受控 bridge、稳定 iframe 和受限 sandbox 暴露
4. 在迁移完成前，保留现有 `WidgetRenderer` 路径作为兼容层

本 RFC 明确：第一波只做 **chat 内 widget host/runtime parity**，不把 dashboard/pin/export/cross-widget 等产品层能力一起拉进来。

## Implementation Snapshot

截至当前代码，仓库已经具备一条可用但仍偏特化的 widget 主链：

1. `WidgetRenderer` 工具可流式输出 HTML，同时返回结构化 `details`：`crates/builtin-tools/src/widget.rs`
2. OpenAI adapter 已支持 `ToolCallArgumentsDelta`，Web 可从参数流中提取半成品 `html`：`crates/openai-adapter/src/{responses,chat_completions}/streaming.rs`
3. server current-turn 已保存 `raw_arguments`、`output_segments` 与最终 `details`：`apps/agent-server/src/session_manager/current_turn.rs`
4. Web 已支持稳定 iframe、`update/finalize` 双阶段渲染、`previewHtml` 缓存、高度同步和基础 bridge：`apps/web/src/features/chat/tool-rendering/renderers/widget.tsx`
5. 当前 capability gating 已开始解耦：`WidgetReadme` / `WidgetRenderer` 仅要求 interactive surface，不再隐式依赖 `Question` capability：`crates/agent-core/src/traits.rs`、`crates/agent-core/src/registry.rs`、`crates/builtin-tools/src/question.rs`
6. 共享核心已落入一组最小 widget 协议类型，用于后续迁移地基：`crates/agent-core/src/widget.rs`

当前仍需注意：这些能力还没有被 runtime / SSE / current-turn / Web store 统一成一套一等 widget 协议，`WidgetRenderer` 仍然是主路径上的特化实现。

## Motivation

当前 `aia` 已经能“渲染 widget”，但它本质上还是：

- 工具参数流/输出流驱动的 HTML 预览
- Web 特判 `WidgetRenderer`
- 通过 iframe 和少量 `postMessage` 做桥接

这条链已经够用，但还不够稳定、可扩展，也不够符合仓库的长期方向：

1. **共享核心优先**：widget 语义不应长期停留在 `apps/web` 特判里
2. **单一内部工具协议**：不应该再长出第二套 Web-only widget 协议
3. **流式稳定性**：widget 的 live / current-turn / replay 应该用同一套共享语义，而不是三份近似投影
4. **宿主安全与边界**：脚本执行、bridge action、主题注入和高度同步都需要更严格的宿主契约

同时，`CodePilot` 已经验证了一套更成熟的方向：

- 更严格的 sandbox/CSP
- 更完整的 bridge action 与宿主生命周期
- 更完整的 CSS bridge
- height cache / finalize lock / scriptsReady
- ErrorBoundary

本 RFC 的目标，是在不破坏当前仓库边界的前提下，把这些成熟能力分阶段吸收到 `aia`。

## Goals

1. 为 widget 宿主定义共享协议，而不是继续把核心语义散在 Web 特判里
2. 把 `WidgetRenderer` 与 `Question` capability 解耦，允许更细粒度的 interactive 能力
3. 为 runtime / server / Web 统一一套 widget 生命周期语义
4. 为后续 sandbox/CSP hardening、CSS bridge、ErrorBoundary、scriptsReady、capture/export 等能力预留稳定接口
5. 在迁移期间保留现有 `WidgetRenderer` 兼容路径，避免历史 turn / current-turn / SSE 回放回归

## Non-Goals

本 RFC 第一波不包含：

1. dashboard/pin/export 产品层能力
2. cross-widget orchestration UI
3. assistant 直接输出 markdown fence widget 的主链替换
4. 桌面端导出宿主或 Electron 隔离窗口
5. 把 widget 演化成通用插件平台

## Proposal

### 1. capability 解耦

当前 `SessionInteractionCapabilities` 保留两个字段：

- `supports_interactive_components`
- `supports_question_tool`

本 RFC 明确这两者不是同义词：

- `supports_interactive_components`：当前 session 是否具备受控交互承接面
- `supports_question_tool`：当前 session 是否允许 runtime 暴露 `Question`

结论：

- `WidgetReadme` / `WidgetRenderer` 只要求 interactive surface
- `Question` 同时要求 interactive surface 与 question capability

### 2. 引入最小共享 widget 协议类型

在 `agent-core` 中引入最小共享类型：

- `UiWidgetPhase`
- `UiWidgetDocument`
- `UiWidget`
- `WidgetHostCommand`
- `WidgetClientEvent`

这些类型先只承担“协议地基”，不立即替换所有现有 Web/store/server 特化字段。

### 3. 渐进迁移，而不是一次切断

迁移顺序：

1. 共享协议与 capability 先落地
2. adapter / runtime / server 把现有 widget 事实映射成共享语义
3. Web host 消费共享语义
4. 旧 `WidgetRenderer` store/renderer fallback 作为兼容层逐步收口

### 4. Web host/runtime 的目标能力

迁移完成后，Web widget host 应至少具备：

- 稳定 iframe receiver
- `update/finalize` 两阶段渲染
- 更严格 sandbox/CSP
- 受控 bridge action
- height cache / finalize lock
- scriptsReady
- ErrorBoundary
- richer CSS bridge

### 5. 产品层能力延后

以下能力保留到后续 RFC / 阶段：

- widget pin / dashboard persistence
- refresh semantics / data contract
- capture/export
- cross-widget publish/subscribe

这些能力必须在基础 widget host/runtime 收稳后再做。

## Alternatives Considered

### A. 继续沿用纯 Web 特判

优点：

- 改得快

缺点：

- 继续把语义堆在 `apps/web`
- live/current-turn/replay 更难统一
- 不符合 shared-core-first

结论：拒绝作为长期方案。

### B. 直接照搬 CodePilot 全套 dashboard/widget 产品层

优点：

- 功能上最接近 CodePilot

缺点：

- 范围炸裂
- 会抢当前 Web↔runtime bridge 收口优先级
- 极易让 `apps/web` / `agent-server` 再次变厚

结论：拒绝作为第一波方案。

## Risks and Mitigations

### 风险 1：共享协议落地后，旧流式路径回归

缓解：

- 保留兼容层
- current-turn / SSE / replay 加 fixture 回归测试

### 风险 2：把 widget 语义重新塞回 `apps/web`

缓解：

- 所有新 widget 协议类型先落在 `agent-core`
- Web 只消费协议，不重新发明第二套 schema

### 风险 3：过早做 dashboard/pin/export

缓解：

- 明确延后到后续阶段
- 第一波只做 host/runtime parity

### 风险 4：bridge/sandbox 收紧后，现有 widget 失效

缓解：

- 保留 feature flag / compatibility shim
- 先引入 schema 校验，再收紧权限

## Open Questions

1. `WidgetHostCommand` / `WidgetClientEvent` 是否在下一阶段直接并入 `StreamEvent`，还是先停留在 Web host/runtime 层消费？
2. capture/export 应该走纯 Web 方案还是桌面宿主方案？
3. cross-widget publish/subscribe 是否在 chat 里也需要，还是只保留给 dashboard？

## Rollout Plan

### Phase 0

- 冻结范围
- 写清边界、兼容窗口与非目标

### Phase 1

- capability 解耦
- 共享 widget 协议最小类型落地

### Phase 2

- adapter / runtime / server 把现有 widget 事实映射成共享语义

### Phase 3

- Web widget host 重构与协议消费

### Phase 4

- sandbox/CSP hardening
- ErrorBoundary
- CSS bridge
- height cache / finalize lock / scriptsReady

### Phase 5+

- capture/export
- cross-widget publish/subscribe
- dashboard persistence / pin / refresh

## Success Criteria

1. widget capability 不再隐式依赖 `Question`
2. `agent-core` 有稳定 widget 协议类型
3. runtime / server / Web 对 widget 语义的 live/current-turn/replay 保持一致
4. Web widget host 具备更严格 bridge/sandbox 和更稳生命周期
5. 第一波不把 dashboard/pin/export 混进基础 host/runtime 改造
