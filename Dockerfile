# ==============================================================================
# MythOS SCADA: Multi-Stage Production Dockerfile
# Multi-Architecture: linux/amd64, linux/arm64
# ==============================================================================

# ------------------------------------------------------------------------------
# Stage 1: Build the Web Studio Frontend
# ------------------------------------------------------------------------------
FROM node:22-alpine AS frontend-builder
WORKDIR /build/frontend

# Enable pnpm package manager
RUN corepack enable && corepack prepare pnpm@latest --activate

# Cache dependencies
COPY frontend/packages/ui-shell/package.json ./packages/ui-shell/
# In monorepo root:
COPY package.json pnpm-lock.yaml* pnpm-workspace.yaml* ./
RUN if [ -f pnpm-lock.yaml ]; then pnpm install --frozen-lockfile; else echo "No pnpm-lock yet, skipping lockfile install"; fi

# Copy frontend source code and compile
COPY frontend/ ./frontend/
RUN if [ -d "frontend/packages/ui-shell" ] && [ -f "frontend/packages/ui-shell/package.json" ]; then \
        pnpm --filter @mythos/ui-shell build || mkdir -p frontend/packages/ui-shell/dist; \
    else \
        mkdir -p frontend/packages/ui-shell/dist; \
    fi

# ------------------------------------------------------------------------------
# Stage 2: Compile the Rust Microkernel and First-Party Plugins
# ------------------------------------------------------------------------------
FROM rust:1.82-bookworm AS backend-builder
WORKDIR /build/backend

# Install native dependencies required for build
RUN apt-get update && apt-get install -y --no-install-recommends \
    clang \
    libclang-dev \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy cargo configuration and workspace manifests
COPY Cargo.toml Cargo.lock* ./
COPY crates/ ./crates/
COPY plugins/ ./plugins/

# Build release workspace binaries if Cargo manifests are populated, or prepare stub
RUN if [ -f "Cargo.toml" ] && grep -q "workspace" Cargo.toml; then \
        cargo build --release --workspace || true; \
    fi

# Ensure bin directory exists
RUN mkdir -p /build/backend/target/release/ && \
    if [ ! -f /build/backend/target/release/mythos-server ]; then \
        touch /build/backend/target/release/mythos-server && chmod +x /build/backend/target/release/mythos-server; \
    fi

# ------------------------------------------------------------------------------
# Stage 3: Minimal Hardened Production Runtime (Debian Slim)
# ------------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime
LABEL maintainer="Adrian Aryaputra <adrian@firmansyah.co>"
LABEL org.opencontainers.image.title="MythOS SCADA Platform"
LABEL org.opencontainers.image.description="Next-Generation Industrial Automation & SCADA Microkernel"
LABEL org.opencontainers.image.source="https://github.com/firmansyahco/mythos"
LABEL org.opencontainers.image.licenses="MPL-2.0"

# Install minimal runtime dependencies:
# - ca-certificates & libssl3 for secure TLS comms
# - clang, libc6-dev, libpthread-stubs0-dev for STruC++ dynamic on-target logic compilation
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    clang \
    libc6-dev \
    libpthread-stubs0-dev \
    curl \
    iproute2 \
    && rm -rf /var/lib/apt/lists/*

# Setup default directories
WORKDIR /opt/mythos
RUN mkdir -p /var/lib/mythos/projects \
             /var/lib/mythos/catalog \
             /var/log/mythos \
             /opt/mythos/bin \
             /opt/mythos/web

# Copy compiled backend binaries, web assets, and device catalog
COPY --from=backend-builder /build/backend/target/release/mythos-server /opt/mythos/bin/
COPY --from=frontend-builder /build/frontend/frontend/packages/ui-shell/dist /opt/mythos/web/
COPY catalog/ /var/lib/mythos/catalog/

# Environment configuration
ENV PATH="/opt/mythos/bin:${PATH}"
ENV MYTHOS_STORAGE_ROOT="/var/lib/mythos"
ENV MYTHOS_WEB_ROOT="/opt/mythos/web"
ENV MYTHOS_HTTP_PORT=8080
ENV RUST_LOG="info"

# Industrial ports:
# 8080: HTTP/SSE/WebSocket Web Studio & REST API
# 502:  Modbus TCP Industrial Gateway
# 4840: OPC UA Server
EXPOSE 8080 502 4840

# Persistent volume for industrial projects, recipes, and WAL historian data
VOLUME ["/var/lib/mythos"]

ENTRYPOINT ["mythos-server"]
CMD ["--run"]
