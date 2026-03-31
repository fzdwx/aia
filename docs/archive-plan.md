# docs archive / notes 约定

> 当前仓库还没有正式建立 `docs/archive/` 目录；本文件先定义迁移规则，后续若继续整理历史文档，可按这里执行。

## 什么时候应该进 archive

满足任一条件即可考虑迁入：

1. 文档主要描述的是**已结束阶段**，不再承担当前真源职责
2. 文档内容以**研究过程、尝试路径、过渡方案**为主，而不是现行边界
3. 文档中的实现事实已经被新的 `status / requirements / architecture / rfc` 明确替代
4. 文档保留有价值，但继续放在 docs 根目录会干扰主阅读路径

## 迁移后建议保留的类型

- 历史阶段路线图
- 已废弃的设计草案
- 过渡期方案说明
- 外部参考摘录或长篇研究笔记

## 不应迁入 archive 的内容

这些仍应留在主 docs 路径：

- `status.md`
- `requirements.md`
- `architecture.md`
- `todo.md`
- `evolution-log.md`
- 当前仍生效的 `rfc/*`
- 当前仍直接服务产品或工程规范的文档

## 当前建议继续观察的文档

这些文档后续可评估是否需要迁入 `docs/archive/` 或 `docs/notes/`：

- `generative-ui-article.md`
- `async-phases.md`（若其“阶段完成”结论已稳定且不再需要频繁引用）
- 未来新增的临时调研稿、对比笔记、一次性方案草案
