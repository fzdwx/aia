# RFC 目录约定

本目录用于存放 `aia` 的设计提案与架构决策文档。

> RFC 用于记录“为什么这样设计”，不是当前实现真源。当前代码状态与优先级请优先看 `docs/status.md`。

- Last verified: `2026-03-30`

RFC 建议使用较 formal 的固定结构，方便后续检索、比较和替换。

## 命名规则

RFC 文件统一使用带序号的文件名：

- `0001-question-runtime-tool.md`
- `0002-some-future-topic.md`

规则如下：

1. 使用四位数字、左侧补零
2. 序号单调递增，不复用旧编号
3. 文件名格式统一为：`NNNN-short-kebab-case-title.md`
4. `short-kebab-case-title` 应简短、稳定、可读

## 分配规则

新增 RFC 时：

1. 先查看本目录下现有最大序号
2. 使用下一个可用序号
3. 不因 RFC 被废弃、替换或撤回而回收旧编号

## 状态建议

常见状态可使用：

- `Draft`
- `Accepted`
- `Implemented`
- `Superseded`
- `Rejected`

## 推荐格式

RFC 建议使用带 `---` 的 frontmatter 元数据块，格式类似：

```md
---
rfc: 0001
name: question-runtime-tool
title: Question Runtime Tool
description: Defines the internal Question runtime tool and its suspend/resume semantics.
status: Draft
date: 2026-03-25
authors:
  - aia
supersedes: []
superseded_by: null
---

# RFC 0001: Question Runtime Tool

## Summary

一句话概述这份 RFC 要解决什么问题、引入什么决策。

## Implementation Snapshot

如果 RFC 已经部分或全部落地，这里用很短的几段话说明：

- 当前哪些部分已经进入代码
- 当前哪些正文段落仍是历史提案或候选方案
- 当前代码真相优先看哪些文件

## Motivation

说明当前痛点、现状缺口和为什么值得做。

## Goals

- ...

## Non-Goals

- ...

## Proposal

给出核心设计、协议、状态机、边界与约束。

## Alternatives Considered

列出考虑过但未采纳的方案，以及拒绝原因。

## Risks and Mitigations

说明主要风险、兼容性问题和缓解方式。

## Open Questions

- ...

## Rollout Plan

分阶段说明如何落地。

## Success Criteria

- ...
```

如果某份 RFC 已进入 `Accepted` / `Implemented`，但正文里仍保留较多设计过程、候选方案或与当前代码不完全一致的草案内容，建议在 `Summary` 后面补一个 `Implementation Snapshot` 小节，明确：

- 当前哪些部分已经落地
- 当前哪些描述只是历史提案 / 候选方案
- 当前代码真相应优先去看哪些文件

字段建议：

- `rfc`: 四位编号字符串或数字
- `name`: 稳定、简短的 kebab-case 标识
- `title`: RFC 标题
- `description`: 一句话概述 RFC 内容，便于索引与目录展示
- `status`: `Draft | Accepted | Implemented | Superseded | Rejected`
- `date`: `YYYY-MM-DD`
- `authors`: 作者列表
- `supersedes`: 被本 RFC 替代或吸收的旧 RFC 列表；没有时用空数组 `[]`
- `superseded_by`: 替代本 RFC 的新 RFC；没有时用 `null`

说明：

- 若 RFC 已落地，可把 `status` 改为 `Implemented`
- 若 RFC 被新提案替代，应同时更新新旧文档的替代关系

## 维护原则

- RFC 用于记录“为什么这样设计”，不替代实现文档
- 已存在 RFC 应以追加修订或新 RFC 替代为主，避免静默改写历史结论
- 若新的 RFC 替代旧 RFC，应在新旧文档里互相标注关系
- 若 RFC 已实现但正文仍保留明显的历史提案痕迹，应优先补 `Implementation Snapshot` 或历史说明，而不是直接把整份 RFC 改写成实现手册
