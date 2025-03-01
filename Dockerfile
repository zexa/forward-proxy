# Build stage
FROM rust:1.76-slim-bullseye AS builder

# Install dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Create a new empty project
WORKDIR /app
COPY . .

# Build the application with optimizations
RUN cargo build --release

# Runtime stage
FROM debian:bullseye-slim

# Create a non-root user to run the proxy
RUN groupadd -r proxy && useradd -r -g proxy proxy

# Install runtime dependencies only
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates netcat-openbsd && \
    rm -rf /var/lib/apt/lists/*

# Create directory for the proxy
WORKDIR /app

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/forward-proxy /app/forward-proxy

# Set file ownership
RUN chown -R proxy:proxy /app

# Switch to the proxy user
USER proxy

# Add healthcheck
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD nc -z 127.0.0.1 8118 || exit 1

# Expose the proxy port
EXPOSE 8118

# Run the proxy
CMD ["/app/forward-proxy"]
