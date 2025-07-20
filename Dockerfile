# ---- Build Stage ----
    FROM rust:latest as builder

    WORKDIR /app
    
    # Install build dependencies
    RUN apt-get update && \
    apt-get install -y \
    protobuf-compiler \
    clang \
    libclang-dev \
    llvm-dev \
    libssl-dev \
    pkg-config
    
    # Copy source and build
    COPY . .
    
    # Build release binaries for both prover and verifier
    RUN cargo build --release --bin prover
    RUN cargo build --release --bin verifier
    
    # ---- Runtime Stage ----
    FROM debian:bullseye-slim
    
    WORKDIR /app
    
    # Install runtime dependencies
    RUN apt-get update && apt-get install -y libssl1.1 ca-certificates && rm -rf /var/lib/apt/lists/*
    
    # Copy binaries from builder
    COPY --from=builder /app/target/release/prover /usr/local/bin/prover
    COPY --from=builder /app/target/release/verifier /usr/local/bin/verifier
    
    # Copy artifacts and config if needed
    COPY artifacts ./artifacts
    COPY .env .env
    
    # Expose default verifier port (change if needed)
    EXPOSE 2025
    
    # Default command (can be overridden)
    CMD ["verifier", "--port", "5789"]