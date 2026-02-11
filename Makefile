# Arvak build automation
#
# Targets:
#   make build          — debug build (fast)
#   make release        — optimized release build
#   make release-lto    — fat-LTO release build
#   make pgo            — PGO-optimized release build (~10-20% faster)
#   make test           — run all tests
#   make bench          — run benchmarks
#   make check          — fmt-check + cargo check + clippy
#   make fmt            — auto-format all code
#   make fmt-check      — verify formatting (fails if unformatted)
#   make preflight      — full pre-push gate (fmt + check + test)
#   make clean          — clean build artifacts
#
# Prerequisites for PGO:
#   rustup component add llvm-tools-preview

CARGO := cargo
RUSTFLAGS_PGO_GEN := -Cprofile-generate=/tmp/arvak-pgo
RUSTFLAGS_PGO_USE := -Cprofile-use=/tmp/arvak-pgo/merged.profdata
LLVM_PROFDATA := $(shell rustup run stable bash -c 'ls $$(rustc --print sysroot)/lib/rustlib/*/bin/llvm-profdata 2>/dev/null | head -1')

.PHONY: build release release-lto pgo test bench check clean fmt fmt-check preflight setup-tooling setup-hooks

build:
	$(CARGO) build

release:
	$(CARGO) build --release

release-lto:
	$(CARGO) build --profile release-lto

# Profile-Guided Optimization: build, profile with benchmarks, rebuild.
# Yields ~10-20% faster binaries for hot compilation paths.
pgo: clean-pgo
	@echo "=== Step 1/4: Instrumented build ==="
	RUSTFLAGS="$(RUSTFLAGS_PGO_GEN)" $(CARGO) build --release --lib -p arvak-ir -p arvak-compile
	@echo "=== Step 2/4: Collecting profile data ==="
	RUSTFLAGS="$(RUSTFLAGS_PGO_GEN)" $(CARGO) test --release -p arvak-ir -p arvak-compile -- --quiet
	RUSTFLAGS="$(RUSTFLAGS_PGO_GEN)" $(CARGO) bench -p arvak-ir --no-run 2>/dev/null || true
	@echo "=== Step 3/4: Merging profiles ==="
	@if [ -z "$(LLVM_PROFDATA)" ]; then \
		echo "ERROR: llvm-profdata not found. Run: rustup component add llvm-tools-preview"; \
		exit 1; \
	fi
	$(LLVM_PROFDATA) merge -o /tmp/arvak-pgo/merged.profdata /tmp/arvak-pgo/
	@echo "=== Step 4/4: Optimized build ==="
	RUSTFLAGS="$(RUSTFLAGS_PGO_USE)" $(CARGO) build --release
	@echo "=== PGO build complete ==="

clean-pgo:
	rm -rf /tmp/arvak-pgo

test:
	$(CARGO) test --workspace --exclude arvak-grpc

bench:
	$(CARGO) bench -p arvak-ir

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

check: fmt-check
	$(CARGO) check --workspace --exclude arvak-grpc
	$(CARGO) clippy --workspace --exclude arvak-grpc -- -D warnings

preflight: fmt-check check test
	@echo "All checks passed — safe to push"

clean:
	$(CARGO) clean
	rm -rf /tmp/arvak-pgo

# Install recommended development tooling.
setup-tooling:
	@echo "Installing pinned toolchain from rust-toolchain.toml..."
	rustup show active-toolchain || rustup install $$(grep channel rust-toolchain.toml | cut -d'"' -f2)
	@echo "Installing mold linker..."
	@which mold > /dev/null 2>&1 || (echo "  -> sudo apt install mold (or brew install mold)" && exit 1)
	@echo "Installing sccache..."
	@which sccache > /dev/null 2>&1 || cargo install sccache
	@echo "Installing cargo-nextest..."
	@which cargo-nextest > /dev/null 2>&1 || cargo install cargo-nextest
	@echo "Installing llvm-tools for PGO..."
	rustup component add llvm-tools-preview
	@echo ""
	@echo "Done! Activate in .cargo/config.toml:"
	@echo "  1. Uncomment the mold linker section"
	@echo "  2. Uncomment the sccache wrapper section"
	@echo "  3. Use 'cargo nextest run' instead of 'cargo test'"
	@echo "  4. Run 'make setup-hooks' to install the pre-push hook"

# Install a git pre-push hook that runs fmt-check before pushing.
setup-hooks:
	@mkdir -p .git/hooks
	@printf '#!/bin/sh\nmake fmt-check\n' > .git/hooks/pre-push
	@chmod +x .git/hooks/pre-push
	@echo "Installed .git/hooks/pre-push (runs make fmt-check)"
