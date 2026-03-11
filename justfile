fmt:
	cargo fmt --all

check:
	cargo check --workspace

test:
	cargo test --workspace

run *ARGS:
	cargo run -p agent-cli -- {{ARGS}}
