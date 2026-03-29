# ── Dashboard Build ────────────────────────────────────────────
# Build the React dashboard first so it's available for rust-embed.
FROM node:22-slim AS dashboard-builder

WORKDIR /dashboard
COPY src/orchestrators/ai/dashboard/package.json src/orchestrators/ai/dashboard/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY src/orchestrators/ai/dashboard/ .
RUN npm run build

# ── Rust Build ────────────────────────────────────────────────
# Each orchestrator is a standalone crate (not in root workspace).
# Docker needs the orchestrator itself, orchestrator-common, and garden-common.
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

# Copy built dashboard into the expected location for rust-embed
COPY --from=dashboard-builder /dashboard/dist/ ./dashboard/dist/

# Build the orchestrator binary
RUN cargo build --profile fast-release --bin zen-garden-ai-orchestrator

# ── Runtime ───────────────────────────────────────────────────
FROM debian:bookworm-slim

# OCI Image Spec metadata
LABEL org.opencontainers.image.title="Zen Garden AI Orchestrator"
LABEL org.opencontainers.image.description="Multi-offering AI service orchestrator for Zen Garden. Routes inference, embedding, TTS, STT, translation, and image generation across heterogeneous AI services with VRAM-aware fitness scoring."
LABEL org.opencontainers.image.vendor="sylin.org"
LABEL org.opencontainers.image.authors="Leo Botinelly <hello@sylin.org>"
LABEL org.opencontainers.image.url="https://github.com/sylin-org/zen-garden"
LABEL org.opencontainers.image.source="https://github.com/sylin-org/zen-garden/tree/main/src/orchestrators/ai"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.base.name="debian:bookworm-slim"
LABEL org.opencontainers.image.version="0.1.0"

# Zen Garden specific metadata
LABEL garden.offering.name="zen-garden.ai.orchestrator"
LABEL garden.offering.category="orchestrator"
LABEL garden.ports.dashboard="7190"
LABEL garden.ports.proxy.ollama="21434"
LABEL garden.ports.proxy.infinity="21438"
LABEL garden.ports.proxy.openedai-speech="21437"
LABEL garden.ports.proxy.libretranslate="21439"
LABEL garden.ports.proxy.speaches="21436"
LABEL garden.ports.proxy.comfyui="21435"

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

# Dashboard + all proxy ports
EXPOSE 7190 21434 21435 21436 21437 21438 21439

ENV AI_ORCH_DATA_DIR=/data

ENTRYPOINT ["/usr/local/bin/zen-garden-ai-orchestrator"]
