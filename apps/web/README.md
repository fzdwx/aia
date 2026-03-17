# aia web

`apps/web` 是当前项目的主界面承接点，也是当前唯一主客户端方向。

## 目标

- 承接 provider 选择、创建、更新、删除与切换
- 承接 session 列表、历史水合、当前运行中轮次恢复
- 承接输入发送、流式 thinking / tool output / assistant text 展示
- 承接 trace loop / span 诊断视图
- 复用 Rust 运行时与 `apps/agent-server` 驱动层，而不是把代理逻辑重写进 React

## 当前状态

- 已接入 `apps/agent-server`，不再是仅承接布局的静态骨架
- 已有 React + TypeScript + Tailwind 基础工程，并切换到 Vite+ 工作流
- 当前使用 `pnpm` 作为锁文件来源，`packageManager` 已声明为 `pnpm`
- 已接入 `shadcn` / 基础 UI 组件体系
- 当前工作台已覆盖：左侧边栏、中央消息区、底部输入区、provider 管理、session 视图、trace 诊断视图
- 已支持流式状态累积、乐观消息渲染、会话历史分页与当前 turn 恢复
- 已支持 stop/cancel 相关前端交互与 cancelled 状态展示
- 已支持主题切换与本地主题状态同步

## 开发规范

- 统一遵循 `docs/frontend-web-guidelines.md`
- 额外遵循 `apps/web/AGENTS.md` 中的 Vite+ / Web 工具链约束
- 前端只负责界面与交互，不重写 Rust 运行时逻辑
- 新基础组件优先基于现有组件体系扩展，不平行引入另一套 UI 基座

## 常用命令

先进入目录：

```bash
cd apps/web
```

如果全局 `vp` 不在当前环境的 `PATH` 中，可使用项目本地 binary：`./node_modules/.bin/vp`。

### 安装与开发

```bash
./node_modules/.bin/vp install
./node_modules/.bin/vp dev
./node_modules/.bin/vp build
./node_modules/.bin/vp preview
```

### 校验

```bash
./node_modules/.bin/vp test
pnpm run test
./node_modules/.bin/tsc --noEmit
pnpm run typecheck
```

其中：

- `./node_modules/.bin/vp test` 是直接调用当前项目本地的 Vite+ 测试入口
- `pnpm run test` 会执行当前 `package.json` 中的 `test` 脚本；按当前仓库配置，它实际运行的是 `bun test`
- `./node_modules/.bin/tsc --noEmit` 与 `pnpm run typecheck` 当前都对应 TypeScript 类型检查

### 当前 `package.json` 脚本语义

- `test`：当前实际执行 `bun test`
- `test:watch`：当前实际执行 `bun test --watch`
- `typecheck`：当前实际执行 `tsc --noEmit`
- `lint` / `format` / `dev`：当前脚本内部仍走 `vp`

在运行命令前，以 `apps/web/package.json` 的当前定义为准，不要假设所有脚本都等价于 `vp` 子命令。

如需运行前后端联调，优先使用仓库根目录下已有的联动命令，而不是在 Web 子目录各自发明流程。
