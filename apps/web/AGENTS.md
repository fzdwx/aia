<!--VITE PLUS START-->

# Using Vite+, the Unified Toolchain for the Web

This project is using Vite+, a unified toolchain built on top of Vite, Rolldown, Vitest, tsdown, Oxlint, Oxfmt, and Vite Task. Vite+ wraps runtime management, package management, and frontend tooling in a single global CLI called `vp`. Vite+ is distinct from Vite, but it invokes Vite through `vp dev` and `vp build`.

## Vite+ Workflow

`vp` is a global binary that handles the full development lifecycle. Run `vp help` to print a list of commands and `vp <command> --help` for information about a specific command.

### Start

- create - Create a new project from a template
- migrate - Migrate an existing project to Vite+
- config - Configure hooks and agent integration
- staged - Run linters on staged files
- install (`i`) - Install dependencies
- env - Manage Node.js versions

### Develop

- dev - Run the development server
- check - Run format, lint, and TypeScript type checks
- lint - Lint code
- fmt - Format code
- test - Run tests

### Execute

- run - Run monorepo tasks
- exec - Execute a command from local `node_modules/.bin`
- dlx - Execute a package binary without installing it as a dependency
- cache - Manage the task cache

### Build

- build - Build for production
- pack - Build libraries
- preview - Preview production build

### Manage Dependencies

Vite+ automatically detects and wraps the underlying package manager such as pnpm, npm, or Yarn through the `packageManager` field in `package.json` or package manager-specific lockfiles.

- add - Add packages to dependencies
- remove (`rm`, `un`, `uninstall`) - Remove packages from dependencies
- update (`up`) - Update packages to latest versions
- dedupe - Deduplicate dependencies
- outdated - Check for outdated packages
- list (`ls`) - List installed packages
- why (`explain`) - Show why a package is installed
- info (`view`, `show`) - View package information from the registry
- link (`ln`) / unlink - Manage local package links
- pm - Forward a command to the package manager

### Maintain

- upgrade - Update `vp` itself to the latest version

These commands map to their corresponding tools. For example, `vp dev --port 3000` runs Vite's dev server and works the same as Vite. `vp test` runs JavaScript tests through the bundled Vitest. The version of all tools can be checked using `vp --version`. This is useful when researching documentation, features, and bugs.

## Common Pitfalls

- **Prefer repo-level `just` wrappers when available:** 在本仓库里，前端相关操作优先使用仓库根目录的 `just web-*` 命令（例如 `just web-install`、`just web-test`、`just web-check`）；只有在你明确要在 `apps/web` 目录内做本地排查，或根 `justfile` 还没有暴露对应入口时，才直接运行本地 `vp` / 脚本命令。
- **Package manager state is still managed by Vite+:** 依赖安装、增删包、锁文件相关操作优先使用 `vp install` / `vp add` / `vp remove`，不要直接用 `pnpm` / `npm` / `yarn` 改依赖状态。
- **Use the commands that actually exist in this package:** 这个目录既有 `vp` 命令，也有直接脚本。当前验证路径里，前端测试可以使用 `vp test`，也可以使用 `pnpm run test`（它会执行当前 `package.json` 里的 `test` 脚本）；TypeScript 类型检查直接使用 `tsc --noEmit`（或 `./node_modules/.bin/tsc --noEmit`）。不要把所有命令都机械改写成同一种入口。
- **`vp` may not be on global `PATH`:** 如果你明确选择在 `apps/web` 目录里直接运行本地命令，先 `cd apps/web`；若 `vp` 不在当前环境的全局 `PATH` 中，则使用项目本地 binary `./node_modules/.bin/vp`。在本仓库里，agent 执行前端校验时优先选择“能明确表达真实工具语义”的命令，并避免依赖宿主机全局安装状态。
- **Running scripts:** `package.json` 里的脚本代表当前目录的真实行为：`test` / `test:watch` 当前已统一走 `vp test --run` / `vp test --watch`，`typecheck` 目前走 `tsc --noEmit`；选择命令前先看 `package.json` 的当前定义，不要假设它们天然等价于某个 `vp` 子命令。
- **Do not install Vitest, Oxlint, Oxfmt, or tsdown directly:** Vite+ wraps these tools. They must not be installed directly. You cannot upgrade these tools by installing their latest versions. Always use Vite+ commands where those tools are actually the intended entrypoint.
- **Use Vite+ wrappers for one-off binaries when appropriate:** 优先用 `vp exec` 执行本地前端工具；如果只是当前仓库已经明确用裸 `tsc`，也可直接使用 `./node_modules/.bin/tsc`。
- **Import JavaScript modules from `vite-plus`:** Instead of importing from `vite` or `vitest`, all modules should be imported from the project's `vite-plus` dependency. For example, `import { defineConfig } from 'vite-plus';` or `import { expect, test, vi } from 'vite-plus/test';`. You must not install `vitest` to import test utilities.
- **Type-Aware Linting:** There is no need to install `oxlint-tsgolint`, `vp lint --type-aware` works out of the box.

## Review Checklist for Agents

- [ ] Run `just web-install` after pulling remote changes and before getting started.
- [ ] Run `just web-check` and `just web-test` to validate changes.
<!--VITE PLUS END-->
