# ── Builder ────────────────────────────────────────────────────
FROM rust:latest AS builder

WORKDIR /build

RUN apt-get update && apt-get install -y \
    ca-certificates \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests first for layer caching
COPY Cargo.toml Cargo.lock* ./
COPY src/common/Cargo.toml src/common/Cargo.toml
COPY src/orchestrators/ollama/Cargo.toml src/orchestrators/ollama/Cargo.toml

# Stub other members so workspace resolves
RUN mkdir -p src/lantern/src && echo "" > src/lantern/src/lib.rs && \
    echo '[package]\nname = "garden-lantern"\nversion = "0.1.0"\nedition = "2021"' > src/lantern/Cargo.toml && \
    mkdir -p src/moss/src && echo "" > src/moss/src/lib.rs && \
    echo '[package]\nname = "garden-moss"\nversion = "0.1.0"\nedition = "2021"' > src/moss/Cargo.toml && \
    mkdir -p src/rake/src && echo "" > src/rake/src/lib.rs && \
    echo '[package]\nname = "garden-rake"\nversion = "0.1.0"\nedition = "2021"' > src/rake/Cargo.toml && \
    mkdir -p src/cricket/src && echo "" > src/cricket/src/lib.rs && \
    echo '[package]\nname = "garden-cricket"\nversion = "0.1.0"\nedition = "2021"' > src/cricket/Cargo.toml && \
    mkdir -p src/firefly/src && echo "" > src/firefly/src/lib.rs && \
    echo '[package]\nname = "garden-firefly"\nversion = "0.1.0"\nedition = "2021"' > src/firefly/Cargo.toml && \
    mkdir -p src/companion-sdk/src && echo "" > src/companion-sdk/src/lib.rs && \
    echo '[package]\nname = "garden-companion-sdk"\nversion = "0.1.0"\nedition = "2021"' > src/companion-sdk/Cargo.toml && \
    mkdir -p src/build-utils/src && echo "" > src/build-utils/src/lib.rs && \
    echo '[package]\nname = "garden-build-utils"\nversion = "0.1.0"\nedition = "2021"' > src/build-utils/Cargo.toml && \
    mkdir -p src/probe/src && echo "" > src/probe/src/lib.rs && \
    echo '[package]\nname = "garden-probe"\nversion = "0.1.0"\nedition = "2021"' > src/probe/Cargo.toml

# Copy real source for orchestrator + common
COPY src/common/ src/common/
COPY src/orchestrators/ollama/ src/orchestrators/ollama/

# Build only the orchestrator binary
RUN cargo build --profile fast-release --bin zen-garden-ollama-orchestrator

# ── Runtime ────────────────────────────────────────────────────
FROM debian:bookworm-slim

# OCI Image Spec metadata
# https://github.com/opencontainers/image-spec/blob/main/annotations.md
LABEL org.opencontainers.image.title="Zen Garden Ollama Orchestrator"
LABEL org.opencontainers.image.description="VRAM-aware multi-instance Ollama orchestrator for Zen Garden. Routes inference requests to the optimal GPU tier, manages model lifecycle, and provides a real-time dashboard."
LABEL org.opencontainers.image.vendor="sylin.org"
LABEL org.opencontainers.image.authors="Leo Botinelly <hello@sylin.org>"
LABEL org.opencontainers.image.url="https://github.com/sylin-org/zen-garden"
LABEL org.opencontainers.image.source="https://github.com/sylin-org/zen-garden/tree/main/src/orchestrators/ollama"
LABEL org.opencontainers.image.documentation="https://github.com/sylin-org/zen-garden/blob/main/docs/DOCUMENTATION.md"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.base.name="debian:bookworm-slim"
LABEL org.opencontainers.image.version="0.1.0"

# Zen Garden specific metadata
LABEL garden.offering.name="zen-garden.ollama.orchestrator"
LABEL garden.offering.category="orchestrator"
LABEL garden.ports.proxy="11434"
LABEL garden.ports.dashboard="7190"

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN useradd -m -u 1000 router
RUN mkdir -p /data && chown router:router /data

COPY --from=builder /build/target/fast-release/zen-garden-ollama-orchestrator /usr/local/bin/zen-garden-ollama-orchestrator
RUN chmod +x /usr/local/bin/zen-garden-ollama-orchestrator

USER router
WORKDIR /data

# Proxy (Ollama-compatible) + Dashboard/Management
EXPOSE 11434 7190

ENV ROUTER_DATA_DIR=/data

ENTRYPOINT ["/usr/local/bin/zen-garden-ollama-orchestrator"]
