# TraderTony V4 - Multi-stage Docker Build
# Optimized for Railway deployment

# =============================================================================
# Stage 1: Build Environment
# =============================================================================
FROM rust:1.75-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release && \
    rm -rf src target/release/trader-tony-v4*

# Copy actual source code
COPY src ./src

# Build the actual application
RUN cargo build --release

# =============================================================================
# Stage 2: Runtime Environment
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 trader
USER trader

# Create data directory for persistence
WORKDIR /app
RUN mkdir -p /app/data

# Copy the compiled binary from builder
COPY --from=builder /app/target/release/trader-tony-v4 /app/trader-tony-v4

# Expose the API port (Railway will set PORT env var)
EXPOSE 3030

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:${PORT:-3030}/api/health || exit 1

# Default environment variables
ENV RUST_LOG=info
ENV API_HOST=0.0.0.0
ENV API_PORT=3030

# Run the application
CMD ["./trader-tony-v4"]
