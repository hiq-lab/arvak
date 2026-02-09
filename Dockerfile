# ============================================================
# Stage 1: Builder
# ============================================================
FROM debian:bookworm-slim AS builder

# Install build essentials, curl, CA certificates, OpenSSL dev headers, and protobuf compiler
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    ca-certificates \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Install Rust nightly via rustup
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain nightly --profile minimal \
    && . /root/.cargo/env && rustc --version
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build

# ----------------------------------------------------------
# Layer 1: Cache dependency compilation
# Copy only manifests and lockfile, create source stubs,
# then build to cache all external dependencies.
# This layer is only invalidated when Cargo.toml/lock changes.
# ----------------------------------------------------------
COPY Cargo.toml Cargo.lock ./

COPY crates/arvak-ir/Cargo.toml crates/arvak-ir/Cargo.toml
COPY crates/arvak-qasm3/Cargo.toml crates/arvak-qasm3/Cargo.toml
COPY crates/arvak-compile/Cargo.toml crates/arvak-compile/Cargo.toml
COPY crates/arvak-hal/Cargo.toml crates/arvak-hal/Cargo.toml
COPY crates/arvak-sched/Cargo.toml crates/arvak-sched/Cargo.toml
COPY crates/arvak-types/Cargo.toml crates/arvak-types/Cargo.toml
COPY crates/arvak-auto/Cargo.toml crates/arvak-auto/Cargo.toml
COPY crates/arvak-dashboard/Cargo.toml crates/arvak-dashboard/Cargo.toml
COPY crates/arvak-cli/Cargo.toml crates/arvak-cli/Cargo.toml
COPY crates/arvak-grpc/Cargo.toml crates/arvak-grpc/Cargo.toml
COPY adapters/arvak-adapter-ibm/Cargo.toml adapters/arvak-adapter-ibm/Cargo.toml
COPY adapters/arvak-adapter-iqm/Cargo.toml adapters/arvak-adapter-iqm/Cargo.toml
COPY adapters/arvak-adapter-qdmi/Cargo.toml adapters/arvak-adapter-qdmi/Cargo.toml
COPY adapters/arvak-adapter-sim/Cargo.toml adapters/arvak-adapter-sim/Cargo.toml
COPY adapters/arvak-adapter-cudaq/Cargo.toml adapters/arvak-adapter-cudaq/Cargo.toml
COPY crates/arvak-eval/Cargo.toml crates/arvak-eval/Cargo.toml
COPY crates/arvak-bench/Cargo.toml crates/arvak-bench/Cargo.toml
COPY demos/Cargo.toml demos/Cargo.toml
COPY demos/lumi-hybrid/Cargo.toml demos/lumi-hybrid/Cargo.toml

# Create stub source files matching each crate's expected targets
RUN mkdir -p crates/arvak-ir/src && echo "" > crates/arvak-ir/src/lib.rs \
    && mkdir -p crates/arvak-qasm3/src && echo "" > crates/arvak-qasm3/src/lib.rs \
    && mkdir -p crates/arvak-compile/src && echo "" > crates/arvak-compile/src/lib.rs \
    && mkdir -p crates/arvak-hal/src && echo "" > crates/arvak-hal/src/lib.rs \
    && mkdir -p crates/arvak-sched/src && echo "" > crates/arvak-sched/src/lib.rs \
    && mkdir -p crates/arvak-types/src && echo "" > crates/arvak-types/src/lib.rs \
    && mkdir -p crates/arvak-auto/src && echo "" > crates/arvak-auto/src/lib.rs \
    && mkdir -p crates/arvak-dashboard/src \
        && echo "fn main() {}" > crates/arvak-dashboard/src/main.rs \
        && echo "" > crates/arvak-dashboard/src/lib.rs \
    && mkdir -p crates/arvak-dashboard/static \
        && touch crates/arvak-dashboard/static/index.html \
        && touch crates/arvak-dashboard/static/app.js \
        && touch crates/arvak-dashboard/static/style.css \
    && mkdir -p crates/arvak-cli/src && echo "fn main() {}" > crates/arvak-cli/src/main.rs \
    && mkdir -p crates/arvak-grpc/src/bin \
        && echo "" > crates/arvak-grpc/src/lib.rs \
        && echo "fn main() {}" > crates/arvak-grpc/src/bin/arvak-grpc-server.rs \
    && mkdir -p crates/arvak-grpc/proto \
        && touch crates/arvak-grpc/proto/arvak.proto \
    && mkdir -p adapters/arvak-adapter-ibm/src && echo "" > adapters/arvak-adapter-ibm/src/lib.rs \
    && mkdir -p adapters/arvak-adapter-iqm/src && echo "" > adapters/arvak-adapter-iqm/src/lib.rs \
    && mkdir -p adapters/arvak-adapter-qdmi/src && echo "" > adapters/arvak-adapter-qdmi/src/lib.rs \
    && mkdir -p adapters/arvak-adapter-sim/src && echo "" > adapters/arvak-adapter-sim/src/lib.rs \
    && mkdir -p adapters/arvak-adapter-cudaq/src && echo "" > adapters/arvak-adapter-cudaq/src/lib.rs \
    && mkdir -p crates/arvak-eval/src && echo "" > crates/arvak-eval/src/lib.rs \
    && mkdir -p crates/arvak-bench/src && echo "" > crates/arvak-bench/src/lib.rs \
    && mkdir -p demos/src && echo "" > demos/src/lib.rs \
    && mkdir -p demos/bin \
        && echo "fn main() {}" > demos/bin/demo_grover.rs \
        && echo "fn main() {}" > demos/bin/demo_vqe.rs \
        && echo "fn main() {}" > demos/bin/demo_qaoa.rs \
        && echo "fn main() {}" > demos/bin/demo_multi.rs \
        && echo "fn main() {}" > demos/bin/demo_all.rs \
        && echo "fn main() {}" > demos/bin/demo_qi_nutshell.rs \
    && mkdir -p demos/lumi-hybrid/src \
        && echo "fn main() {}" > demos/lumi-hybrid/src/main.rs \
        && echo "fn main() {}" > demos/lumi-hybrid/src/quantum_worker.rs \
    && mkdir -p demos/data \
        && echo "{}" > demos/data/vqe_result.json

# Feature flags for the dashboard build
ARG DASHBOARD_FEATURES="with-simulator"

# Build dependencies only (this layer is cached until Cargo.toml/Cargo.lock change)
RUN cargo build --release --workspace --exclude arvak-python 2>/dev/null || true

# ----------------------------------------------------------
# Layer 2: Build actual project source
# ----------------------------------------------------------
RUN rm -rf crates/ adapters/ demos/ examples/

COPY crates/ crates/
COPY adapters/ adapters/
COPY demos/ demos/
COPY examples/ examples/

# Ensure cargo detects real sources as newer than cached stubs
RUN find crates/ adapters/ demos/ -name "*.rs" -exec touch {} +

# Build dashboard, CLI, gRPC server, and demos
RUN cargo build --release -p arvak-dashboard --features "${DASHBOARD_FEATURES}"
RUN cargo build --release -p arvak-cli
RUN cargo build --release -p arvak-grpc --features "simulator,sqlite"
RUN cargo build --release -p arvak-demos -p lumi-hybrid

# ============================================================
# Stage 2: Runtime (minimal)
# ============================================================
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN groupadd --gid 1000 arvak && \
    useradd --uid 1000 --gid arvak --create-home arvak

# Copy binaries from builder
COPY --from=builder /build/target/release/arvak-dashboard /usr/local/bin/arvak-dashboard
COPY --from=builder /build/target/release/arvak /usr/local/bin/arvak
COPY --from=builder /build/target/release/arvak-grpc-server /usr/local/bin/arvak-grpc-server
COPY --from=builder /build/target/release/demo-grover /usr/local/bin/demo-grover
COPY --from=builder /build/target/release/demo-vqe /usr/local/bin/demo-vqe
COPY --from=builder /build/target/release/demo-qaoa /usr/local/bin/demo-qaoa
COPY --from=builder /build/target/release/demo-multi /usr/local/bin/demo-multi
COPY --from=builder /build/target/release/demo-all /usr/local/bin/demo-all
COPY --from=builder /build/target/release/demo-qi-nutshell /usr/local/bin/demo-qi-nutshell
COPY --from=builder /build/target/release/lumi_vqe /usr/local/bin/lumi_vqe
COPY --from=builder /build/target/release/quantum_worker /usr/local/bin/quantum_worker

# Copy example QASM files
COPY examples/*.qasm /home/arvak/examples/

RUN chown -R arvak:arvak /home/arvak

USER arvak
WORKDIR /home/arvak

# Default environment for dashboard
ENV ARVAK_BIND=0.0.0.0:3000
ENV RUST_LOG=arvak=info,tower_http=info

EXPOSE 3000 50051 9090

CMD ["arvak-dashboard"]
