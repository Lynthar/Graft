# Graft Dockerfile
# Multi-stage build for minimal image size

# Stage 1: Build frontend
FROM node:20-alpine AS web-builder
WORKDIR /app/web
COPY web/package*.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# Stage 2: Build backend
FROM rust:1.83-alpine AS rust-builder
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig

WORKDIR /app

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy actual source
COPY src/ ./src/
COPY migrations/ ./migrations/

# Copy frontend build
COPY --from=web-builder /app/web/dist ./web/dist

# Build the actual binary
RUN touch src/main.rs && cargo build --release

# Stage 3: Final image
FROM alpine:3.20

RUN apk add --no-cache ca-certificates tzdata

WORKDIR /app

# Copy binary
COPY --from=rust-builder /app/target/release/graft /app/graft

# Create data directory
RUN mkdir -p /app/data

# Environment variables
ENV GRAFT_DATA_DIR=/app/data
ENV GRAFT_HOST=0.0.0.0
ENV GRAFT_PORT=3000
ENV RUST_LOG=graft=info

# Expose port
EXPOSE 3000

# Volume for persistent data
VOLUME ["/app/data"]

# Run
CMD ["/app/graft"]
