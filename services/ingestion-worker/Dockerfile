# Ingestion Worker Dockerfile
FROM rust:1.84-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    cmake \
  && rm -rf /var/lib/apt/lists/*

# Copy source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build the application
RUN cargo build --release --locked

# Final stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    libgcc-s1 \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /root/

# Copy the binary from builder
COPY --from=builder /app/target/release/ingestion-worker .

# Entry point wrapper for startup diagnostics
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Run the binary
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
