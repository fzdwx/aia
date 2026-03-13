fmt:
	cargo fmt --all

check:
	cargo check --workspace

test:
	cargo test --workspace

run *ARGS:
	cargo run -p agent-cli -- {{ARGS}}

# 同时启动后端和前端
dev:
	#!/usr/bin/env bash
	set -e
	cargo run -p agent-server &
	SERVER_PID=$!
	cd apps/web && bun dev &
	WEB_PID=$!
	trap "kill $SERVER_PID $WEB_PID 2>/dev/null" EXIT
	wait

# 只启动后端
dev-server:
	cargo run -p agent-server

# 只启动前端
dev-web:
	cd apps/web && bun dev

# TypeScript 类型检查
typecheck:
	cd apps/web && bun run typecheck
