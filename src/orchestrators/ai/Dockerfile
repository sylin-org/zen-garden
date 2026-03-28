# ── Stage 1: Build React Dashboard ─────────────────────────────
FROM node:22-slim AS web-builder

WORKDIR /web
COPY src/orchestrators/ai/web/package.json src/orchestrators/ai/web/package-lock.json ./
RUN npm ci --ignore-scripts

COPY src/orchestrators/ai/web/ .
RUN npm run build

# ── Stage 2: Build Rust Binary ─────────────────────────────────
# Each orchestrator is a standalone crate. Docker only needs the
# orchestrator itself, orchestrator-common, and garden-common.
FROM rust:latest AS builder

WORKDIR /build/src/orchestrators/ai

RUN apt-get update && apt-get install -y \
    ca-certificates \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace root Cargo.toml (garden-common uses edition.workspace = true)
COPY Cargo.toml /build/Cargo.toml

# Copy only the crates we actually depend on
COPY src/common/ /build/src/common/
COPY src/orchestrators/common/ /build/src/orchestrators/common/
COPY src/orchestrators/ai/ .

# Copy the built React dashboard into web/dist/ so rust-embed picks it up
COPY --from=web-builder /web/dist/ ./web/dist/

# Build the orchestrator binary
RUN cargo build --profile fast-release --bin zen-garden-ai-orchestrator

# ── Stage 3: Runtime ───────────────────────────────────────────
FROM debian:bookworm-slim

# OCI Image Spec metadata
LABEL org.opencontainers.image.title="Zen Garden AI Orchestrator"
LABEL org.opencontainers.image.description="Unified AI capability router for Zen Garden. Manages Ollama, ComfyUI, whisper.cpp, OpenedAI Speech, Infinity, LibreTranslate, and cloud providers across distributed compute nodes."
LABEL org.opencontainers.image.vendor="sylin.org"
LABEL org.opencontainers.image.authors="Leo Botinelly <hello@sylin.org>"
LABEL org.opencontainers.image.url="https://github.com/sylin-org/zen-garden"
LABEL org.opencontainers.image.source="https://github.com/sylin-org/zen-garden/tree/main/src/orchestrators/ai"
LABEL org.opencontainers.image.documentation="https://github.com/sylin-org/zen-garden/blob/main/docs/DOCUMENTATION.md"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.base.name="debian:bookworm-slim"
LABEL org.opencontainers.image.version="0.1.0"

# Zen Garden specific metadata
LABEL garden.offering.name="zen-garden.ai.orchestrator"
LABEL garden.offering.category="orchestrator"
LABEL garden.ports.proxy="21434"
LABEL garden.ports.dashboard="7190"

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN useradd -m -u 1000 router
RUN mkdir -p /data && chown router:router /data

COPY --from=builder /build/src/orchestrators/ai/target/fast-release/zen-garden-ai-orchestrator /usr/local/bin/zen-garden-ai-orchestrator
RUN chmod +x /usr/local/bin/zen-garden-ai-orchestrator

USER router
WORKDIR /data

# Proxy (Ollama-compatible + extensions) + Dashboard/Management
EXPOSE 21434 7190

ENV ZG_DATA_DIR=/data

ENTRYPOINT ["/usr/local/bin/zen-garden-ai-orchestrator"]
