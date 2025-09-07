# Multi-stage build for optimized final image
FROM rust:1.83-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    gcc \
    libc-dev

# Create app directory
WORKDIR /app

# Copy manifest files
COPY Cargo.toml Cargo.lock ./

# Create src directory and a dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies first (this layer will be cached)
RUN cargo build --release && rm src/main.rs

# Copy source code
COPY src ./src

# Build the application
# Touch main.rs to ensure it's rebuilt
RUN touch src/main.rs && \
    cargo build --release && \
    strip target/release/mksub-rs

# Final stage - minimal runtime image
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    libgcc

# Create non-root user
RUN addgroup -g 1000 mksub && \
    adduser -u 1000 -G mksub -s /bin/sh -D mksub

# Copy binary from builder stage
COPY --from=builder /app/target/release/mksub-rs /usr/local/bin/mksub-rs

# Make binary executable
RUN chmod +x /usr/local/bin/mksub-rs

# Switch to non-root user
USER mksub

# Set working directory
WORKDIR /data

# Set default entrypoint
ENTRYPOINT ["/usr/local/bin/mksub-rs"]

# Default command shows help
CMD ["--help"]

# Labels
LABEL org.opencontainers.image.title="mksub-rs"
LABEL org.opencontainers.image.description="Ultra-fast subdomain generator written in Rust"
LABEL org.opencontainers.image.url="https://robensive.in"
LABEL org.opencontainers.image.source="https://github.com/robensive/mksub-rs"
LABEL org.opencontainers.image.vendor="robensive.in"
LABEL org.opencontainers.image.licenses="MIT"
