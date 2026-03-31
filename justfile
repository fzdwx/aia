fmt:
	cargo fmt --all

check:
	cargo check --workspace

test:
	cargo test --workspace

# 安装前端依赖（优先复用本地 Vite+，首次引导时回退到 pnpm）
web-install:
	cd apps/web && pnpm install

# 启动前端开发服务器
web-dev:
	cd apps/web && pnpm run dev

# 构建前端生产包
web-build:
	cd apps/web && pnpm run build

# 预览前端生产包
web-preview:
	cd apps/web && pnpm run preview

# 前端 lint
web-lint:
	cd apps/web && pnpm run lint

# 前端格式化
web-format:
	cd apps/web && pnpm run format

# 前端类型检查
web-typecheck:
	cd apps/web && pnpm run typecheck

# 前端测试
web-test:
	cd apps/web && pnpm run test

# 前端测试（watch）
web-test-watch:
	cd apps/web && pnpm run test:watch

# 前端全量检查
web-check:
	#!/usr/bin/env bash
	set -e
	if [ ! -x apps/web/node_modules/.bin/vp ]; then
		echo "apps/web/node_modules/.bin/vp 不存在，请先运行 just web-install" >&2
		exit 1
	fi
	cd apps/web && ./node_modules/.bin/vp check

# 同时启动后端和前端
dev: web-install
	#!/usr/bin/env bash
	set -e
	cargo run --release -p agent-server  &
	SERVER_PID=$!
	cd apps/web && pnpm run dev &
	WEB_PID=$!
	trap "kill $SERVER_PID $WEB_PID 2>/dev/null" EXIT
	wait

# 只启动后端
dev-server:
	cargo run -p agent-server

# 只启动前端
dev-web:
	just web-dev

# TypeScript 类型检查
typecheck:
	just web-typecheck
