# ── Stage 1: Build frontend ──────────────────────────────────────
FROM node:25-alpine3.22 AS frontend-build
WORKDIR /app/frontend
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ .
RUN npm run build
# Output: /app/frontend/dist/

# ── Stage 2: Build backend ───────────────────────────────────────
FROM rust:1.90-bookworm AS backend-build
WORKDIR /app/backend
COPY backend/Cargo.toml backend/Cargo.lock ./
# Cache dependencies layer
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release && rm -rf src
COPY backend/ .
ENV SQLX_OFFLINE=true
RUN cargo build --release

# ── Stage 3: Runtime ─────────────────────────────────────────────
FROM debian:bookworm-slim
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
