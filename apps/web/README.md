# aia web

`apps/web` 是当前项目的主界面承接点，也是当前唯一主客户端方向。

## 目标

- 承接 provider 选择、创建、更新、删除与切换
- 承接 channel（当前仅飞书）创建、更新、删除与启停配置
- 承接 session 列表、历史水合、当前运行中轮次恢复
- 承接输入发送、流式 thinking / tool output / assistant text 展示
- 承接 trace loop / span 诊断视图
- 复用 Rust 运行时与 `apps/agent-server` 驱动层，而不是把代理逻辑重写进 React

## 当前状态

- 已接入 `apps/agent-server`，不再是仅承接布局的静态骨架
- 已有 React + TypeScript + Tailwind 基础工程，并切换到 Vite+ 工作流
- 当前使用 `pnpm` 作为锁文件来源，`packageManager` 已声明为 `pnpm`
- 已接入 `shadcn` / 基础 UI 组件体系
- 当前工作台已覆盖：左侧边栏、中央消息区、底部输入区、provider 管理、channel 配置、session 视图、trace 诊断视图
- 已支持流式状态累积、乐观消息渲染、会话历史分页与当前 turn 恢复
- 已支持 stop/cancel 相关前端交互与 cancelled 状态展示
- 已支持主题切换与本地主题状态同步
- 聊天与推理区 Markdown 渲染现已切到 `markstream-react`，继续保留流式更新与代码块/表格等富文本能力

## 开发规范

- 统一遵循 `docs/frontend-web-guidelines.md`
- 额外遵循 `apps/web/AGENTS.md` 中的 Vite+ / Web 工具链约束
- 前端只负责界面与交互，不重写 Rust 运行时逻辑
- 新基础组件优先基于现有组件体系扩展，不平行引入另一套 UI 基座

## 常用命令

前端相关命令统一优先通过仓库根目录 `just` 运行。

### 安装与开发

```bash
just web-install
just web-dev
just web-build
just web-preview
just dev
just dev-web
```

### 校验

```bash
just web-lint
just web-format
just web-typecheck
just web-test
just web-test-watch
just web-check
just typecheck
```

如需直接在 `apps/web` 目录内排查问题，再回退到本地命令：

```bash
cd apps/web
./node_modules/.bin/vp dev
./node_modules/.bin/vp build
./node_modules/.bin/vp test
./node_modules/.bin/tsc --noEmit
```

其中：

- `just web-dev` / `just web-build` / `just web-test` 等命令会通过仓库根目录 `justfile` 调用 `apps/web/package.json` 的当前脚本语义
- `./node_modules/.bin/vp test` 是直接调用当前项目本地的 Vite+ 测试入口
- `./node_modules/.bin/tsc --noEmit` 当前对应 TypeScript 类型检查

### 当前 `package.json` 脚本语义

- `test`：当前实际执行 `vp test --run`
- `test:watch`：当前实际执行 `vp test --watch`
- `typecheck`：当前实际执行 `tsc --noEmit`
- `lint` / `format` / `dev`：当前脚本内部仍走 `vp`

在运行命令前，以 `apps/web/package.json` 的当前定义为准，不要假设所有脚本都等价于 `vp` 子命令。

如需运行前后端联调，优先使用仓库根目录下已有的 `just dev`，而不是在 Web 子目录各自发明流程。
