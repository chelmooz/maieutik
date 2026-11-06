FROM rust:1.99 as builder

# Install build deps
RUN apt-get update && apt-get install -y \
    cmake \
    clang \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy source
COPY . /app
WORKDIR /app

# Build
RUN cargo build --release --locked --features cpp-modules

# Runtime image
FROM debian:bookworm-slim

# Install runtime deps
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries
COPY --from=builder /app/target/release/maieutik /usr/local/bin/
COPY --from=builder /app/target/release/maieutik-api /usr/local/bin/

# Default config
ENV MAIEUTIK_LLM_MODEL="ollama://qwen2.5:3b"
ENV MAIEUTIK_INSTANCE_NAME="maieutik"

WORKDIR /data
VOLUME ["/data"]

CMD ["maieutik", "index"]