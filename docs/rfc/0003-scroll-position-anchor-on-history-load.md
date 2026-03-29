---
rfc: 0003
name: scroll-position-anchor-on-history-load
title: Scroll Position Anchor on History Load
description: Fixes the scroll position jumping issue when loading older messages and improves loading UX with dynamic trigger threshold and visual feedback.
status: Implemented
date: 2026-03-29
authors:
  - aia
supersedes: []
superseded_by: null
---

# RFC 0003: Scroll Position Anchor on History Load

## Summary

修复向上滚动加载历史消息时滚动位置跳动问题，同时优化加载体验：
1. 记录加载前的 scrollHeight 和 scrollTop，加载后恢复位置
2. 使用视口高度比例（1.5x）触发预加载，适配不同屏幕尺寸
3. 改进加载指示器，添加旋转动画提供视觉反馈

## Motivation

当前实现在 `handleLoadOlderTurns` 时使用 `scrollAnchorRef` 记录 `container.scrollHeight`，加载完成后通过计算高度差调整 `scrollTop`：

```ts
// 当前实现
scrollAnchorRef.current = container.scrollHeight  // 记录旧高度
// 加载完成
const added = container.scrollHeight - anchor
if (added > 0) container.scrollTop += added
```

这种方式存在问题：

1. **定位不准确**：高度差计算是近似值，无法精确定位到用户原本查看的消息
2. **视觉跳动**：用户会看到视图"跳"到新加载内容的最后一条，而不是停留在原来的位置
3. **依赖假设**：假设新内容均匀添加在顶部，忽略了动态高度变化（如图片加载、代码块渲染等）

正确的行为应该是：加载完成后，用户原本看到的第一条消息仍然出现在视图的相同位置。

## Goals

1. 向上加载历史消息后，保持用户视图相对于第一条可见消息的位置不变
2. 支持动态高度内容的正确锚定
3. 与现有的自动跟随底部逻辑保持兼容
4. 不引入额外的 DOM 查询开销（在必要时才查询）

## Non-Goals

1. 不处理向下加载（当前没有此功能）
2. 不处理窗口大小变化时的滚动位置（已有 ResizeObserver 处理）
3. 不引入虚拟列表（virtual list）机制

## Proposal

### 核心思路

将锚点从"高度差"改为"第一条可见消息元素"：

1. **加载前**：找到当前视图中第一条可见的消息元素，记录其 `turn_id`
2. **加载后**：通过 `turn_id` 找到该元素的最新 DOM 引用，将其滚动到视图顶部

### 实现方案

#### 1. 数据属性标记

确保每个 turn 的 DOM 元素有 `data-turn-id` 属性：

```tsx
// 在 TurnView 组件中
<div data-turn-id={turn.turn_id} ...>
  ...
</div>
```

#### 2. 锚点记录

修改 `scrollAnchorRef` 的类型和记录逻辑：

```ts
// 之前
const scrollAnchorRef = useRef<number | null>(null)

// 之后
const scrollAnchorRef = useRef<string | null>(null)  // 存储 turn_id
```

```ts
const handleLoadOlderTurns = useCallback(async () => {
  if (historyLoadingMore || sessionHydrating || !historyHasMore) return
  const container = containerRef.current
  if (!container) return

  // 找到第一条可见的消息
  const firstVisibleTurnId = findFirstVisibleTurn(container)
  scrollAnchorRef.current = firstVisibleTurnId
  autoFollowRef.current = false

  await loadOlderTurns()
}, [loadOlderTurns, historyLoadingMore, sessionHydrating, historyHasMore])
```

#### 3. 查找第一条可见消息

```ts
function findFirstVisibleTurn(container: HTMLElement): string | null {
  const turns = container.querySelectorAll('[data-turn-id]')
  const containerRect = container.getBoundingClientRect()

  for (const turn of turns) {
    const rect = turn.getBoundingClientRect()
    // 元素顶部在容器视口内
    if (rect.bottom > containerRect.top && rect.top < containerRect.bottom) {
      return turn.getAttribute('data-turn-id')
    }
  }
  return null
}
```

#### 4. 恢复滚动位置

修改 ResizeObserver 回调：

```ts
useEffect(() => {
  const content = contentRef.current
  if (!content) return

  const resizeObserver = new ResizeObserver(() => {
    const container = containerRef.current
    if (!container) return

    // 处理历史消息加载后的滚动锚定
    const anchorTurnId = scrollAnchorRef.current
    if (anchorTurnId !== null) {
      scrollAnchorRef.current = null
      const anchorElement = container.querySelector(
        `[data-turn-id="${anchorTurnId}"]`
      )
      if (anchorElement) {
        anchorElement.scrollIntoView({ block: 'start' })
        return
      }
    }

    if (autoFollowRef.current) {
      scrollToBottom()
    }
  })

  resizeObserver.observe(content)
  return () => {
    resizeObserver.disconnect()
  }
}, [activeSessionId, scrollToBottom])
```

### 边界情况

1. **锚点元素不存在**：如果 `turn_id` 对应的元素找不到（极端情况），回退到当前行为
2. **加载失败**：`scrollAnchorRef.current` 在下次加载时会被覆盖
3. **快速连续加载**：新加载会覆盖旧锚点，这是预期行为

### 与现有逻辑的兼容

- `autoFollowRef` 逻辑保持不变：用户向上滚动时设为 `false`，滚到底部时设为 `true`
- `scrollToBottom` 函数保持不变
- `isAtBottom` 状态计算保持不变

## Alternatives Considered

### 方案 A：保持当前的高度差计算

不采纳。原因：
- 高度差计算不够精确
- 无法处理动态内容导致的实际高度变化
- 用户体验差，视图会跳动

### 方案 B：使用 `scrollIntoView` + `block: 'start'` + 偏移量

可以考虑，但需要额外处理容器内的滚动：

```ts
anchorElement.scrollIntoView({ block: 'start' })
// 如果需要调整偏移
container.scrollTop -= headerOffset
```

当前方案已足够，偏移量可以在后续迭代中添加。

### 方案 C：使用 `overflow-anchor: auto` CSS 特性

浏览器原生的 scroll anchoring 可以自动处理内容插入时的滚动位置。但：
- 需要验证浏览器兼容性
- 当前代码已设置 `[overflow-anchor:none]`，可能与现有逻辑冲突
- 控制粒度不如手动实现精确

可以作为后续优化方向，但当前问题需要更可控的解决方案。

## Risks and Mitigations

### 1. DOM 查询性能

风险：每次加载前需要遍历 DOM 元素查找第一条可见消息。

缓解：
- 只在触发加载时查询一次
- 使用 `querySelectorAll` + 遍历，复杂度为 O(n)，n 为当前渲染的消息数
- 可以考虑缓存或使用 `IntersectionObserver` 优化，但当前规模下非必要

### 2. 动态内容高度变化

风险：图片、代码块等动态加载后高度变化，可能影响锚定效果。

缓解：
- 锚定到消息元素的顶部，而非中间位置
- ResizeObserver 会在内容尺寸变化时再次触发，但此时锚点已清空
- 如需更精确控制，可以记录相对于消息顶部的偏移量

### 3. 快速滚动时的竞态条件

风险：用户快速滚动可能导致多次加载触发。

缓解：
- `historyLoadingMore` 状态已防止重复加载
- 新加载会覆盖旧锚点，符合用户最新意图

## Rollout Plan

### Phase 1：添加 data-turn-id 属性

- 在 `TurnView` 或相关组件添加 `data-turn-id` 属性
- 确保该属性稳定且与 `turn.turn_id` 一致

### Phase 2：实现锚点逻辑

- 添加 `findFirstVisibleTurn` 辅助函数
- 修改 `handleLoadOlderTurns` 记录锚点
- 修改 ResizeObserver 回调恢复滚动位置

### Phase 3：测试与验证

- 手动测试各种滚动场景
- 验证与自动跟随底部的兼容性
- 验证快速加载、加载失败等边界情况

## Success Criteria

1. 向上滚动加载历史消息后，用户原本看到的第一条消息仍出现在视图顶部
2. 无视觉跳动或闪烁
3. 与现有的"自动跟随底部"行为无冲突
4. 不影响正常滚动和新消息到达时的自动滚动
