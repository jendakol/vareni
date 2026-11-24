# Vareni

Self-hosted recipe manager (Czech UI): recipes, meal plan, eating log, AI chat, recipe discovery.
Rust/Axum backend (`backend/`, crate `cooking-app`), Vue 3 + Vite + Tailwind 4 frontend (`frontend/`),
PostgreSQL 18 + pgvector. README is in Czech and is the user-facing source of truth; `SPEC.md` is the
original design and partly outdated (still mentions JWT login and push).

## Commands

```bash
# Backend (from backend/)
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test                 # integration tests start Postgres via testcontainers -> Docker required
cargo run                  # needs DATABASE_URL + .env, migrations run on startup

# Frontend (from frontend/)
npm ci && npm run dev      # Vite on :5173, proxies to :8080
npm run build              # vue-tsc + vite build, output served by the backend from static/

# Whole stack
docker compose up -d
```

Docker image pins Rust 1.90 and builds with `edition = "2024"`.

## Layout

- `backend/src/routes/` — HTTP handlers; `db/` — sqlx queries (runtime queries, no `query!` macros);
  `ai/` — Claude client, ingest, chat, plan, discovery; `scraper.rs` — 14 curated sites registered in
  `providers()`; `browser.rs` — headless Chromium with stealth evasions; `embedding.rs` — ONNX.
- `backend/migrations/` — numbered, append-only (`NNN_name.sql`).
- `backend/tests/` — one file per area; `common/mod.rs` seeds two users and logs them in with real sessions.
- `docs/superpowers/specs/` — design specs. `docs/superpowers/plans/` is gitignored, keep it that way.

## Auth

Keycloak OIDC (code + PKCE); server-side session in an HttpOnly cookie (`__Host-vareni_session`, plain
`vareni_session` only when `ENVIRONMENT=local`). Users are matched by `users.external_id` = OIDC `sub`
and must carry `OIDC_REQUIRED_ROLE`. State-changing requests need `X-CSRF-Token`.
`POST /api/log` uses a static Bearer `LOG_API_TOKEN` instead. The `debug-auth` cargo feature adds a
dev-login and only works with `ENVIRONMENT=local`.
`cooking-app create-session|revoke-sessions <user>` are break-glass CLI commands.

## Rules

- **No real secrets anywhere in the repo or docs**: no real passwords, hosts, tokens, connection strings
  in code, specs, plans, README, commit messages. Use placeholders (`example.com`, `change-me`).
  Test-only keys live in `backend/tests/fixtures/` and are documented there.
- Run `cargo fmt` and `cargo clippy` before every commit; keep tests green.
- Keep `.env.example`, README config table and `config.rs` in sync when adding or removing env vars.
- UI strings and README are Czech; code, comments and commit messages are English.
- Comments: brief, only for non-obvious why.
- Conventional commits (`feat(scope): ...`, `fix:`, `docs:`, `chore:`, `build:`).
