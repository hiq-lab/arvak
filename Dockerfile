# ============================================================
# Stage 1: Builder
# ============================================================
FROM debian:bookworm-slim AS builder

# Install build essentials, curl, CA certificates, and OpenSSL dev headers
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    curl \
    ca-certificates \
    pkg-config \
    libssl-dev \
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
# then build to cache all 361 external dependencies.
# This layer is only invalidated when Cargo.toml/lock changes.
# ----------------------------------------------------------
COPY Cargo.toml Cargo.lock ./

COPY crates/hiq-ir/Cargo.toml crates/hiq-ir/Cargo.toml
COPY crates/hiq-qasm3/Cargo.toml crates/hiq-qasm3/Cargo.toml
COPY crates/hiq-compile/Cargo.toml crates/hiq-compile/Cargo.toml
COPY crates/hiq-hal/Cargo.toml crates/hiq-hal/Cargo.toml
COPY crates/hiq-sched/Cargo.toml crates/hiq-sched/Cargo.toml
COPY crates/hiq-types/Cargo.toml crates/hiq-types/Cargo.toml
COPY crates/hiq-auto/Cargo.toml crates/hiq-auto/Cargo.toml
COPY crates/hiq-dashboard/Cargo.toml crates/hiq-dashboard/Cargo.toml
COPY crates/hiq-cli/Cargo.toml crates/hiq-cli/Cargo.toml
COPY adapters/hiq-adapter-ibm/Cargo.toml adapters/hiq-adapter-ibm/Cargo.toml
COPY adapters/hiq-adapter-iqm/Cargo.toml adapters/hiq-adapter-iqm/Cargo.toml
COPY adapters/hiq-adapter-qdmi/Cargo.toml adapters/hiq-adapter-qdmi/Cargo.toml
COPY adapters/hiq-adapter-sim/Cargo.toml adapters/hiq-adapter-sim/Cargo.toml
COPY demos/Cargo.toml demos/Cargo.toml
COPY demos/lumi-hybrid/Cargo.toml demos/lumi-hybrid/Cargo.toml

# Create stub source files matching each crate's expected targets
RUN mkdir -p crates/hiq-ir/src && echo "" > crates/hiq-ir/src/lib.rs \
    && mkdir -p crates/hiq-qasm3/src && echo "" > crates/hiq-qasm3/src/lib.rs \
    && mkdir -p crates/hiq-compile/src && echo "" > crates/hiq-compile/src/lib.rs \
    && mkdir -p crates/hiq-hal/src && echo "" > crates/hiq-hal/src/lib.rs \
    && mkdir -p crates/hiq-sched/src && echo "" > crates/hiq-sched/src/lib.rs \
    && mkdir -p crates/hiq-types/src && echo "" > crates/hiq-types/src/lib.rs \
    && mkdir -p crates/hiq-auto/src && echo "" > crates/hiq-auto/src/lib.rs \
    && mkdir -p crates/hiq-dashboard/src \
        && echo "fn main() {}" > crates/hiq-dashboard/src/main.rs \
        && echo "" > crates/hiq-dashboard/src/lib.rs \
    && mkdir -p crates/hiq-dashboard/static \
        && touch crates/hiq-dashboard/static/index.html \
        && touch crates/hiq-dashboard/static/app.js \
        && touch crates/hiq-dashboard/static/style.css \
    && mkdir -p crates/hiq-cli/src && echo "fn main() {}" > crates/hiq-cli/src/main.rs \
    && mkdir -p adapters/hiq-adapter-ibm/src && echo "" > adapters/hiq-adapter-ibm/src/lib.rs \
    && mkdir -p adapters/hiq-adapter-iqm/src && echo "" > adapters/hiq-adapter-iqm/src/lib.rs \
    && mkdir -p adapters/hiq-adapter-qdmi/src && echo "" > adapters/hiq-adapter-qdmi/src/lib.rs \
    && mkdir -p adapters/hiq-adapter-sim/src && echo "" > adapters/hiq-adapter-sim/src/lib.rs \
    && mkdir -p demos/src && echo "" > demos/src/lib.rs \
    && mkdir -p demos/bin \
        && echo "fn main() {}" > demos/bin/demo_grover.rs \
        && echo "fn main() {}" > demos/bin/demo_vqe.rs \
        && echo "fn main() {}" > demos/bin/demo_qaoa.rs \
        && echo "fn main() {}" > demos/bin/demo_multi.rs \
        && echo "fn main() {}" > demos/bin/demo_all.rs \
    && mkdir -p demos/lumi-hybrid/src \
        && echo "fn main() {}" > demos/lumi-hybrid/src/main.rs \
        && echo "fn main() {}" > demos/lumi-hybrid/src/quantum_worker.rs

# Feature flags for the dashboard build
ARG DASHBOARD_FEATURES="with-simulator"

# Build dependencies only (this layer is cached until Cargo.toml/Cargo.lock change)
RUN cargo build --release --workspace --exclude hiq-python 2>/dev/null || true

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

# Build dashboard, CLI, and demos
RUN cargo build --release -p hiq-dashboard --features "${DASHBOARD_FEATURES}"
RUN cargo build --release -p hiq-cli
RUN cargo build --release -p hiq-demos -p lumi-hybrid

# ============================================================
# Stage 2: Runtime (minimal)
# ============================================================
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN groupadd --gid 1000 hiq && \
    useradd --uid 1000 --gid hiq --create-home hiq

# Copy binaries from builder
COPY --from=builder /build/target/release/hiq-dashboard /usr/local/bin/hiq-dashboard
COPY --from=builder /build/target/release/hiq /usr/local/bin/hiq
COPY --from=builder /build/target/release/demo-grover /usr/local/bin/demo-grover
COPY --from=builder /build/target/release/demo-vqe /usr/local/bin/demo-vqe
COPY --from=builder /build/target/release/demo-qaoa /usr/local/bin/demo-qaoa
COPY --from=builder /build/target/release/demo-multi /usr/local/bin/demo-multi
COPY --from=builder /build/target/release/demo-all /usr/local/bin/demo-all
COPY --from=builder /build/target/release/lumi_vqe /usr/local/bin/lumi_vqe
COPY --from=builder /build/target/release/quantum_worker /usr/local/bin/quantum_worker

# Copy example QASM files
COPY examples/*.qasm /home/hiq/examples/

RUN chown -R hiq:hiq /home/hiq

USER hiq
WORKDIR /home/hiq

# Bind to all interfaces so Docker port mapping works
ENV HIQ_BIND=0.0.0.0:3000
ENV RUST_LOG=hiq_dashboard=info,tower_http=info

EXPOSE 3000

CMD ["hiq-dashboard"]
