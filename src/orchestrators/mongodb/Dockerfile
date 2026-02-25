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
COPY src/orchestrators/common/Cargo.toml src/orchestrators/common/Cargo.toml
COPY src/orchestrators/mongodb/Cargo.toml src/orchestrators/mongodb/Cargo.toml

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
    echo '[package]\nname = "garden-probe"\nversion = "0.1.0"\nedition = "2021"' > src/probe/Cargo.toml && \
    mkdir -p src/orchestrators/ollama/src && echo "" > src/orchestrators/ollama/src/lib.rs && \
    echo '[package]\nname = "zen-garden-ollama-orchestrator"\nversion = "0.1.0"\nedition = "2021"' > src/orchestrators/ollama/Cargo.toml

# Copy real source for orchestrator + dependencies
COPY src/common/ src/common/
COPY src/orchestrators/common/ src/orchestrators/common/
COPY src/orchestrators/mongodb/ src/orchestrators/mongodb/

# Build only the orchestrator binary
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

COPY --from=builder /build/target/fast-release/zen-garden-mongodb-orchestrator /usr/local/bin/zen-garden-mongodb-orchestrator
RUN chmod +x /usr/local/bin/zen-garden-mongodb-orchestrator

USER router
WORKDIR /data

# Dashboard/Management
EXPOSE 7191

ENV MONGODB_ORCH_DATA_DIR=/data

ENTRYPOINT ["/usr/local/bin/zen-garden-mongodb-orchestrator"]
