# aia web

`apps/web` 是当前项目的主界面承接点。

## 目标

- 承接 provider 选择与创建
- 承接会话时间线与流式输出
- 承接输入发送、会话恢复与运行状态展示
- 复用 Rust 运行时与共享 driver，而不是把代理逻辑重写进 React

## 当前状态

- 已有 Vite + React + TypeScript 基础工程
- 已替换掉模板首页，改为项目主界面工作台骨架
- 已接入一批 `shadcn` 基础组件（card、badge、input、textarea、separator、scroll-area）
- 当前页面结构已收敛为：左侧边栏、中央消息列表、底部输入框
- 还未接入 Rust 运行时桥接，当前页面主要用于承接后续 Web 交互布局

## 开发规范

- 统一遵循 `docs/frontend-web-guidelines.md`
- 前端只负责界面与交互，不重写 Rust 运行时逻辑
- 新基础组件优先使用 `shadcn` 作为起点

## 常用命令

```bash
bun install
bun run dev
bun run build
```
