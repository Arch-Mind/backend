# Ingestion Worker Dockerfile
FROM rust:1.84-alpine AS builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./

# Create dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Build the application
RUN cargo build --release

# Final stage
FROM alpine:latest

RUN apk --no-cache add ca-certificates libgcc

WORKDIR /root/

# Copy the binary from builder
COPY --from=builder /app/target/release/ingestion-worker .

# Run the binary
CMD ["./ingestion-worker"]
