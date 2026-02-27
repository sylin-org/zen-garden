# ── Builder ────────────────────────────────────────────────────
# Each orchestrator is a standalone crate (no workspace). Docker only needs
# to copy the orchestrator itself, orchestrator-common, and garden-common.
# Zero stubs required.
FROM rust:latest AS builder

# Mirror the repo layout so path deps (../../common) resolve correctly
WORKDIR /build/src/orchestrators/mongodb

RUN apt-get update && apt-get install -y \
    ca-certificates \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy only the crates we actually depend on
COPY src/common/ /build/src/common/
COPY src/orchestrators/common/ /build/src/orchestrators/common/
COPY src/orchestrators/mongodb/ .

# Build the orchestrator binary
RUN cargo build --profile fast-release --bin zen-garden-mongodb-orchestrator

# ── Runtime ────────────────────────────────────────────────────
FROM debian:bookworm-slim

# OCI Image Spec metadata
# https://github.com/opencontainers/image-spec/blob/main/annotations.md
LABEL org.opencontainers.image.title="Zen Garden MongoDB Orchestrator"
LABEL org.opencontainers.image.description="Replica set orchestrator for MongoDB offerings in Zen Garden. Auto-discovers instances, bootstraps replica sets, monitors health/oplog/cache, and provides a management dashboard."
LABEL org.opencontainers.image.vendor="sylin.org"
LABEL org.opencontainers.image.authors="Leo Botinelly <hello@sylin.org>"
LABEL org.opencontainers.image.url="https://github.com/sylin-org/zen-garden"
LABEL org.opencontainers.image.source="https://github.com/sylin-org/zen-garden/tree/main/src/orchestrators/mongodb"
LABEL org.opencontainers.image.documentation="https://github.com/sylin-org/zen-garden/blob/main/docs/DOCUMENTATION.md"
LABEL org.opencontainers.image.licenses="MIT"
LABEL org.opencontainers.image.base.name="debian:bookworm-slim"
LABEL org.opencontainers.image.version="0.1.0"

# Zen Garden specific metadata
LABEL garden.offering.name="zen-garden.mongodb.orchestrator"
LABEL garden.offering.category="orchestrator"
LABEL garden.ports.dashboard="7191"

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user
RUN useradd -m -u 1000 router
RUN mkdir -p /data && chown router:router /data

COPY --from=builder /build/src/orchestrators/mongodb/target/fast-release/zen-garden-mongodb-orchestrator /usr/local/bin/zen-garden-mongodb-orchestrator
RUN chmod +x /usr/local/bin/zen-garden-mongodb-orchestrator

USER router
WORKDIR /data

# Dashboard/Management
EXPOSE 7191

ENV MONGODB_ORCH_DATA_DIR=/data

ENTRYPOINT ["/usr/local/bin/zen-garden-mongodb-orchestrator"]
