---
rfc: 0003
name: session-history-scroll-stability
title: Session History Scroll Stability
description: Defines stable scroll anchoring and bottom-follow semantics for session history hydration, markdown reflow, and paginated upward loading in the Web chat timeline.
status: Draft
date: 2026-03-28
authors:
  - aia
supersedes: []
superseded_by: null
---

# RFC 0003: `Session` 历史加载滚动稳定性

## Summary

为 `aia` 的 Web 聊天时间线引入一套更稳定的 session 历史加载与滚动语义：在初次进入 session、历史分页上翻、以及 Markdown/代码块/图片等异步重排场景下，不再依赖“直接写 `scrollTop = scrollHeight`”或“仅用容器高度差补偿”这类脆弱策略，而是统一收口为基于底部哨兵与可见锚点的滚动恢复机制。该方案要解决两个当前已知问题：一是当历史消息包含 Markdown 富内容时，自动滚动到底部并不能真正贴到最终底部；二是用户向上滚动时若速度较快，在加载下一页历史的过程中视口会被错误顶到最上方。

## Motivation

当前 Web 聊天区已经具备：

- session 切换后自动跳到最新消息底部
- 上滚接近顶部时自动加载前一页历史
- 通过 `ResizeObserver` 在内容尺寸变化后做一定程度的滚动补偿

这套机制在纯文本、固定高度内容下基本可用，但在真实消息流里已经暴露出两个根因级问题。

### 1. 初次贴底与流式贴底对富内容不稳定

当消息里包含以下内容时：

- Markdown 标题/列表重排
- 代码块高亮
- Mermaid / 数学公式
- 图片尺寸晚于首帧确定
- 字体加载或容器宽度变化导致的二次换行

DOM 的最终高度并不是一次渲染就稳定的。

当前实现虽然会在 `ResizeObserver` 中，如果 `autoFollowRef.current` 为真就再次调用 `scrollToBottom()`，但这仍然属于“内容每变化一次，就猜一次底部”的被动策略。它有两个问题：

- “是否真的到底”依赖某次回调时机，而不是依赖一个稳定的底部锚点
- 富内容的多阶段重排可能在多帧里发生，导致某一次贴底后，后续又冒出新的高度增长，最终停在“接近底部但没到底”的状态

用户体感就是：明明应该自动滚到底，结果底下还剩一截，尤其在长 Markdown 或复杂代码块里更明显。

### 2. 上翻分页时视口恢复对快速滚动不稳定

当前上翻分页的核心做法是：

1. 触发加载前记录 `container.scrollHeight`
2. 新历史插入后，用 `新的 scrollHeight - 旧的 scrollHeight` 补到 `scrollTop`

这在“加载期间用户基本不动”的前提下成立，但如果用户滚动很快，问题就来了：

- 用户在旧分页尚未完成时继续向上拖动
- 新内容插入后，补偿仍然基于旧的总高度差，而不是基于用户当前可见内容
- 如果这时又叠加 Markdown 重排或提示条高度变化，视口很容易被推到不正确的位置

最极端的表现就是自动跳到最顶上，像是浏览器丢失了上下文锚点。

### 根因总结

这两个问题本质上都不是“阈值调得不对”，而是当前滚动恢复语义选错了基准：

- 贴底场景不应该用“把 `scrollTop` 设成 `scrollHeight`”当作最终语义，而应该用“让底部哨兵重新进入视口”当作语义
- 上翻分页不应该用“总高度差”当作恢复基准，而应该用“保持当前可见锚点相对视口的位置不变”当作语义

也就是说，当前问题不是单点 bug，而是缺少统一的滚动锚定模型。

## Goals

- 让 session 初次水合后稳定贴到真正底部，而不是只到某个中间重排阶段的底部
- 让流式输出或富内容晚到重排时，底部跟随保持稳定，不出现“差一点没到底”
- 让向上滚动加载历史时，不因用户快速滚动而跳到最顶端
- 让历史分页恢复基于“可见锚点”而不是“高度差猜测”
- 统一 session 切换、首次加载、分页补页、富内容重排这几类滚动语义，避免每条路径各修各的
- 为后续消息虚拟化或更复杂布局保留稳定语义边界

## Non-Goals

- 本 RFC 不引入新的聊天列表虚拟化方案
- 本 RFC 不改动历史分页的服务端协议与 page token 语义
- 本 RFC 不试图解决所有浏览器原生滚动锚定差异，只定义应用层的权威恢复模型
- 本 RFC 不在本轮引入“记住每个 session 的中间滚动位置并精确恢复”能力
- 本 RFC 不修改 Markdown 渲染器本身，只约束它对滚动层的协作方式

## Proposal

### 1. 统一三种滚动意图

聊天区的滚动不应再被实现细节驱动，而应显式区分三类意图：

- `follow_bottom`
- `preserve_anchor`
- `no_auto_adjust`

语义如下：

- `follow_bottom`：当前用户被认为处于“跟随最新消息”模式，内容变化后应让底部哨兵重新可见
- `preserve_anchor`：当前正在向上翻页或恢复分页，内容变化后应保持用户正在看的那条消息在视口中的相对位置稳定
- `no_auto_adjust`：用户主动浏览历史且不在分页恢复窗口内，普通内容变化不应抢滚动权

当前代码里的 `autoFollowRef` 已经隐含表达了第一类，但还缺失第二类的显式状态，所以补偿逻辑只能退化为一次性的 `scrollHeight` 差值修正。

本 RFC 建议把聊天滚动状态收口为一个显式的小状态机，而不是继续依赖多个 ref 之间的隐式组合。

建议状态示意：

- `FollowingBottom`
- `PreservingHistoryAnchor`
- `BrowsingHistory`

迁移建议：

- session 切换成功后，进入 `FollowingBottom`
- 用户主动向上滚离底部阈值后，进入 `BrowsingHistory`
- 触发历史补页时，进入 `PreservingHistoryAnchor`
- 分页恢复完成且用户仍未回到底部时，回到 `BrowsingHistory`
- 任何时刻如果用户重新回到底部阈值内，回到 `FollowingBottom`

### 2. 底部语义改为“底部哨兵可见”，而不是 `scrollTop = scrollHeight`

当前 DOM 已经在内容尾部放了一个空元素：

- `apps/web/src/features/chat/chat-messages/index.tsx`

它现在更多只是配合浏览器滚动锚定使用，没有成为真正的“权威底部目标”。

本 RFC 建议明确引入底部哨兵元素概念：

- `bottomSentinel`

在 `FollowingBottom` 状态下，所有自动贴底操作都不再直接写：

- `container.scrollTop = container.scrollHeight`

而改为一种“让底部哨兵进入视口”的统一动作，例如：

- 优先使用 `bottomSentinel.scrollIntoView({ block: "end" })`
- 若需要更精细控制，再退回基于元素相对位置计算的容器滚动

这么做的关键好处不是 API 漂亮，而是语义稳定：

- 底部内容高度如何变化，不重要
- Markdown 重新排版几次，也不重要
- 只要仍处于 `FollowingBottom`，系统就持续保证“底部那个元素在视口里”

这比反复猜 `scrollHeight` 更接近用户真正想要的结果。

### 2.1 `FollowingBottom` 需要短暂的“稳定窗口”

只靠一次 `scrollIntoView` 还不够。

因为富内容经常是分阶段长高的：

- 首帧文本出来
- 下一帧代码块样式生效
- 再下一帧图片高度确定

本 RFC 建议在进入 `FollowingBottom` 的关键场景时，增加一个短暂的“底部稳定窗口”，例如 `150~300ms` 或若干连续 animation frame，在窗口内只要检测到底部内容尺寸继续变化，就重复把底部哨兵拉回视口，直到满足稳定条件再结束。

稳定条件建议不要写成“只执行一次”，而应类似：

- 连续 `N` 帧底部位置不再变化
- 或窗口超时

这样才能真正覆盖 Markdown 晚到重排。

### 3. 上翻分页改为基于“首个可见锚点”的恢复

当前上翻分页前记录的是：

- `scrollAnchorRef.current = container.scrollHeight`

这不够稳定。

本 RFC 建议改为在触发 `loadOlderTurns()` 前，记录一个明确的可见锚点：

- 当前视口中第一个可见 turn 的 `turn_id`
- 该锚点元素相对容器顶部的偏移量

建议命名为：

- `HistoryViewportAnchor`

示意结构：

```json
{
  "turn_id": "turn_123",
  "offset_top": 84
}
```

恢复逻辑改为：

1. 触发补页前找到首个可见 turn 元素
2. 记录它的 `turn_id`
3. 记录它相对容器顶部的像素偏移
4. 历史页插入后重新找到同一个 `turn_id` 对应元素
5. 调整容器滚动，使它回到原先相同的偏移位置

这样恢复依据从“整页长了多少”变成“我刚才看的那条消息还在原来那儿”，对快速滚动和富内容重排都更稳。

### 3.1 分页恢复窗口内要锁住锚点所有权

仅仅记录锚点还不够，分页恢复期间还需要一条明确约束：

- 当状态是 `PreservingHistoryAnchor` 时，普通 `ResizeObserver` 回调不允许抢占为 `follow_bottom`

否则现在这种情况还会继续发生：

- 用户已在看历史
- 分页刚插入新内容
- 某个尺寸变化回调误以为应该自动贴底或重复补偿
- 结果把视口拉乱

因此，本 RFC 要求在 `PreservingHistoryAnchor` 状态持续期间：

- 底部跟随逻辑暂停
- 只允许锚点恢复逻辑生效
- 直到锚点恢复完成后再退出该状态

### 4. 历史加载触发要去重，并与“正在恢复锚点”解耦

当前已存在：

- `historyLoadingMore`

它能防止重复发请求，但还不能防止滚动事件在恢复期间反复把用户带回触发阈值附近，然后立刻又命中新一轮加载判断。

本 RFC 建议补一层更明确的前端约束：

- `PreservingHistoryAnchor` 状态下，禁止再次触发新的自动补页判定

也就是说，“请求完成”与“视口恢复完成”不是同一个时刻。

只有当：

- 数据已插入
- 锚点已恢复
- 滚动状态已重新结算

这三件事都结束后，才允许下一次上翻自动加载。

这样可以避免用户快速连拖时，同一临界位置触发多次竞态恢复。

### 5. 每个 turn 需要稳定 DOM 锚点

如果要按 `turn_id` 恢复视口，每个 turn 对应的 DOM 就必须有稳定可查询标识。

本 RFC 建议：

- 每个 `TurnView` 根节点暴露 `data-turn-id="..."`

这样聊天滚动层可以在不侵入具体消息内容结构的前提下，稳定查找锚点元素。

这条约束应由聊天列表层统一消费，不应让 Markdown 子节点、tool timeline 子节点成为恢复锚点，因为它们高度变化更频繁，也更容易失去稳定 identity。

### 6. 浏览器原生 `overflow-anchor` 继续保持禁用为主

当前容器已经显式使用：

- `[overflow-anchor:none]`

这个方向是对的，应继续保持为应用层权威。

原因是：

- 原生锚定对简单文档流很有帮助
- 但在聊天这种“底部跟随 + 顶部分页 + 流式增长 + 富内容重排”混合场景里，浏览器并不知道哪一个元素才是业务语义上的权威锚点

本 RFC 明确：

- 聊天主滚动容器继续禁用浏览器默认锚定
- 应用层自己维护 `bottomSentinel` 与 `HistoryViewportAnchor`

### 7. 将滚动恢复从“单次补偿”升级为“直到稳定”

无论是 `FollowingBottom` 还是 `PreservingHistoryAnchor`，都不应该只做一次恢复。

因为真正让滚动出错的，恰恰是这些晚到变化：

- Markdown 组件第二次布局
- 图片 onload
- 代码块样式注入
- status/hint 区块高度变动

因此本 RFC 建议统一抽象一个滚动稳定器，例如：

- `ScrollStabilizer`

它不关心“为什么恢复”，只关心：

- 当前目标是跟底部，还是保锚点
- 本帧恢复后，目标是否稳定
- 如果还不稳定，继续下一帧
- 到超时或连续稳定后结束

这样才能避免不同路径各写一套 `requestAnimationFrame` / `ResizeObserver` 的局部补丁。

## Alternatives Considered

### 1. 继续沿用 `scrollHeight` 差值补偿，只修阈值

这是最小改动方案，但只能缓解，不能解根因。

问题不在于阈值，而在于基准错了。总高度差并不能表达“用户刚才正在看哪条消息”。

### 2. 完全依赖浏览器原生滚动锚定

这对普通文档流可行，但对聊天时间线不够可靠。

浏览器不知道：

- 什么时候应该贴底
- 什么时候应该保顶部锚点
- 什么时候用户是在主动浏览历史

这些都是产品语义，不是浏览器能替我们猜对的。

### 3. 立刻引入完整虚拟化列表

虚拟化也许最终值得做，但它不是这个问题的直接答案。

如果滚动语义没先定义清楚，虚拟化只会把问题带进更复杂的测量和回收逻辑里。先收口锚定模型，再决定是否做虚拟化，更稳。

## Risks and Mitigations

### 1. 风险：状态机变复杂，出现新的边界态

缓解方式：

- 把滚动语义收口成少量显式状态，而不是继续靠多个 ref 隐式组合
- 为 `FollowingBottom` / `PreservingHistoryAnchor` 分别补行为测试

### 2. 风险：稳定窗口过长，导致滚动“黏住”

缓解方式：

- 稳定窗口只在明确的自动恢复场景开启
- 一旦检测到用户主动滚动，应立即打断当前稳定器

### 3. 风险：按 `turn_id` 找锚点时，目标元素尚未挂载

缓解方式：

- 恢复逻辑允许跨若干帧重试
- 若超时仍未找到，则降级为最接近的前后锚点或保守不动

### 4. 风险：多个尺寸变化源叠加，恢复逻辑反复抢滚动权

缓解方式：

- 明确恢复所有权：同一时刻只能有一个稳定器持有滚动控制权
- `PreservingHistoryAnchor` 优先级高于普通贴底

## Open Questions

- `FollowingBottom` 的稳定窗口更适合用“连续稳定帧数”还是“固定超时 + 帧轮询”的组合？
- 首个可见锚点是否应该按“首个部分可见 turn”还是“可见面积最大的 turn”来定义？
- 若历史分页返回后锚点 turn 已被压缩或不在当前列表里，降级策略应优先选前一个 turn 还是后一个 turn？
- 是否需要把“用户主动滚动”的判定与触摸板惯性滚动区分开？

## Rollout Plan

### Phase 1: 收口滚动语义与 DOM 锚点

- 为聊天滚动层引入显式状态：`FollowingBottom / PreservingHistoryAnchor / BrowsingHistory`
- 为每个 turn 根节点补稳定 `data-turn-id`
- 把当前 `scrollHeight` 差值锚点记录替换为 `HistoryViewportAnchor`

### Phase 2: 落地底部哨兵与锚点恢复稳定器

- 把自动贴底统一改为“底部哨兵可见”语义
- 引入短暂稳定窗口，覆盖 Markdown/图片/代码块晚到重排
- 把上翻分页恢复改为按 `turn_id + offset_top` 恢复

### Phase 3: 补测试与边界态验证

- 为“初次加载含 Markdown 历史后应真正贴底”补前端测试
- 为“上翻时快速连续滚动，不应跳到最顶端”补前端测试
- 验证 session 切换、流式输出、历史补页三条主路径不会互相抢滚动权

## Success Criteria

- 打开包含长 Markdown/代码块历史的 session 后，消息区稳定落在最终底部，而不是停在距离底部数十像素的位置
- 用户向上快速滚动并触发多次历史补页时，视口不再被顶到最上方
- 历史页插入后，用户原本正在看的首个可见 turn 保持在近似相同的视口位置
- 聊天区的自动贴底与历史补页恢复逻辑在代码层有统一状态模型，而不是继续散落在多个 ref 和一次性补偿分支中
