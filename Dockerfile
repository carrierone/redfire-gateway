# Multi-stage build for Redfire Gateway
# Optimized for production deployment

# Build stage
FROM rust:1.70-slim as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    libpcap-dev \
    libsnmp-dev \
    pkg-config \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /usr/src/redfire-gateway

# Copy Cargo files first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source files to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "// dummy lib" > src/lib.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release && \
    rm -rf src/

# Copy source code
COPY src/ ./src/
COPY examples/ ./examples/

# Build the actual application
RUN touch src/main.rs && \
    cargo build --release --bin redfire-gateway

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libpcap0.8 \
    libsnmp40 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create redfire user
RUN groupadd -r redfire && useradd -r -g redfire redfire

# Create directories
RUN mkdir -p /etc/redfire /var/log/redfire-gateway /var/lib/redfire-gateway && \
    chown -R redfire:redfire /etc/redfire /var/log/redfire-gateway /var/lib/redfire-gateway

# Copy binary from builder stage
COPY --from=builder /usr/src/redfire-gateway/target/release/redfire-gateway /usr/local/bin/

# Copy example configurations
COPY --from=builder /usr/src/redfire-gateway/examples/ /etc/redfire/examples/

# Copy documentation
COPY README.md CHANGELOG.md INTEGRATION.md LICENSE-GPL /usr/share/doc/redfire-gateway/

# Set proper permissions
RUN chmod +x /usr/local/bin/redfire-gateway && \
    chown -R redfire:redfire /etc/redfire/examples

# Switch to non-root user
USER redfire

# Set working directory
WORKDIR /home/redfire

# Expose ports
EXPOSE 5060/udp 5060/tcp 161/udp 8080/tcp

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD /usr/local/bin/redfire-gateway --version || exit 1

# Default command
CMD ["/usr/local/bin/redfire-gateway", "--config", "/etc/redfire/gateway.toml"]

# Build metadata
ARG VERSION="1.0.0-beta.1"
ARG BUILD_DATE
ARG VCS_REF

LABEL \
    org.opencontainers.image.title="Redfire Gateway" \
    org.opencontainers.image.description="High-performance TDM over Ethernet to SIP gateway" \
    org.opencontainers.image.version="${VERSION}" \
    org.opencontainers.image.created="${BUILD_DATE}" \
    org.opencontainers.image.revision="${VCS_REF}" \
    org.opencontainers.image.source="https://github.com/redfire/redfire-gateway" \
    org.opencontainers.image.url="https://github.com/redfire/redfire-gateway" \
    org.opencontainers.image.documentation="https://github.com/redfire/redfire-gateway/blob/main/README.md" \
    org.opencontainers.image.vendor="Redfire Team" \
    org.opencontainers.image.licenses="GPL-3.0-or-later" \
    org.opencontainers.image.ref.name="${VERSION}" \
    maintainer="Redfire Team <team@redfire.com>"