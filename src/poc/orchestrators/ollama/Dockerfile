# ── Builder ────────────────────────────────────────────────────
# Each orchestrator is a standalone crate (no workspace). Docker only needs
# to copy the orchestrator itself, orchestrator-common, and garden-common.
# Zero stubs required.
FROM rust:latest AS builder

# Mirror the repo layout so path deps (../../common) resolve correctly
WORKDIR /build/src/orchestrators/ollama

RUN apt-get update && apt-get install -y \
    ca-certificates \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace root Cargo.toml (garden-common uses edition.workspace = true)
COPY Cargo.toml /build/Cargo.toml

# Copy only the crates we actually depend on
COPY src/common/ /build/src/common/
COPY src/orchestrators/common/ /build/src/orchestrators/common/
COPY src/orchestrators/ollama/ .

# Build the orchestrator binary
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

COPY --from=builder /build/src/orchestrators/ollama/target/fast-release/zen-garden-ollama-orchestrator /usr/local/bin/zen-garden-ollama-orchestrator
RUN chmod +x /usr/local/bin/zen-garden-ollama-orchestrator

USER router
WORKDIR /data

# Proxy (Ollama-compatible) + Dashboard/Management
EXPOSE 11434 7190

ENV ROUTER_DATA_DIR=/data

ENTRYPOINT ["/usr/local/bin/zen-garden-ollama-orchestrator"]
