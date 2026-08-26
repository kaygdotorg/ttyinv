NIX_RUST ?= RUSTUP_TOOLCHAIN= nix shell nixpkgs\#rustc nixpkgs\#cargo nixpkgs\#rustfmt -c
RUST_FILES := $(sort $(wildcard apps/cli/src/*.rs apps/cli/tests/*.rs crates/ttyinv-core/src/*.rs crates/ttyinv-core/tests/*.rs crates/ttyinv-wasm/src/*.rs))

.PHONY: test check rust-check rust-release wasm format schema clean parity

test:
	$(NIX_RUST) cargo test --workspace

rust-check:
	$(NIX_RUST) cargo check --workspace

rust-release:
	$(NIX_RUST) cargo build --workspace --release

wasm:
	$(NIX_RUST) cargo check -p ttyinv-wasm --target wasm32-unknown-unknown

format:
	$(NIX_RUST) rustfmt --check $(RUST_FILES)

schema:
	$(NIX_RUST) cargo run -q -p ttyinv-cli -- schema --output /tmp/ttyinv-v2.schema.json
	diff -u schema/ttyinv-v2.schema.json /tmp/ttyinv-v2.schema.json

check: test rust-check format schema

parity: test
	@echo "JSON and YAML adapters use the same typed Document."

clean:
	$(NIX_RUST) cargo clean
