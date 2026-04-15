# ── Stage 1: Build frontend ──────────────────────────────────────
FROM node:25-alpine3.22 AS frontend-build
WORKDIR /app/frontend
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build
# Output: /app/frontend/dist/

# ── Stage 2: Build backend ───────────────────────────────────────
# Trixie (Debian 13) required: ort's ONNX Runtime binaries need glibc ≥ 2.38
# (Bookworm ships 2.36, missing __isoc23_strto* symbols)
FROM debian:trixie AS backend-build
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl build-essential pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.90.0
ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /app/backend
COPY backend/Cargo.toml backend/Cargo.lock ./
# Cache dependencies layer
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release && rm -rf src
COPY backend/ .
ENV SQLX_OFFLINE=true
RUN cargo build --release

# ── Stage 3: Runtime ─────────────────────────────────────────────
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    chromium \
    fonts-liberation \
    && rm -rf /var/lib/apt/lists/*

ENV CHROME_PATH=/usr/bin/chromium
WORKDIR /app
COPY --from=backend-build /app/backend/target/release/cooking-app ./
COPY --from=frontend-build /app/frontend/dist/ ./static/
COPY backend/migrations/ ./migrations/

EXPOSE 8080
VOLUME ["/app/uploads"]
CMD ["./cooking-app"]
