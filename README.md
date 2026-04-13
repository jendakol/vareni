# Vareni

Self-hosted aplikace pro dva uživatele na správu receptů, plánování jídel a zaznamenávání, co jsme jedli. Používá Claude
API jako AI jádro.

## Funkcionalita

- **Recepty** — přidávání z textu, fotek (i více najednou) nebo URL. AI extrahuje ingredience, postup a metadata.
  Vyhledávání přes názvy, ingredience i tagy. Emoji, sdílení přes veřejný odkaz.
- **Plán** — navrhování jídel na x dní s ohledem na dietní omezení a historii. Potvrzování a mazání návrhů.
- **Log** — zaznamenávání co kdo jedl, per-user (každý zvlášť nebo oba). Editace a mazání záznamů, navigace po dnech.
- **Chat** — úprava receptu přes konverzaci s AI.
- **Ingestion** — automatické rozpoznání receptu z textu, fotek i webových stránek.

## Tech stack

| Vrstva     | Technologie                   |
|------------|-------------------------------|
| Backend    | Rust / Axum                   |
| Databáze   | PostgreSQL 18 + pgvector      |
| Frontend   | Vue 3 + Vite + Tailwind CSS 4 |
| AI         | Anthropic Claude API          |
| DB přístup | sqlx (async)                  |

## Spuštění

### Požadavky

- Rust (stable)
- Node.js 18+
- Docker (pro PostgreSQL)

### 1. Databáze

```bash
docker compose up -d
```

### 2. Konfigurace

```bash
cp .env.example .env
# Vyplňte ANTHROPIC_API_KEY a JWT_SECRET
```

### 3. Backend

```bash
cd backend
cargo run
```

Server běží na `http://localhost:8080`. Migrace se spustí automaticky při startu.

### 4. Frontend (vývoj)

```bash
cd frontend
npm install
npm run dev
```

Vite dev server běží na `http://localhost:5173` s proxy na backend.

### 5. Frontend (produkce)

```bash
cd frontend
npm run build
```

Výsledek je v `frontend/dist/`, backend ho servuje přímo.

## Docker

Multi-stage build — frontend i backend se zkompilují v jednom image.

### Sestavení

```bash
docker build -t vareni .
```

### Spuštění

```bash
docker run -d \
  --name vareni \
  -p 8080:8080 \
  -v vareni-uploads:/app/uploads \
  --env-file .env \
  vareni
```

Aplikace potřebuje PostgreSQL s rozšířením pgvector — buď přes `docker compose up -d` (image `pgvector/pgvector:pg18`,
pro lokální vývoj), nebo vlastní instanci.  
Proměnná `DATABASE_URL` v `.env` musí ukazovat na dostupnou databázi.

Image obsahuje frontend (statické soubory), backend a migrace. Vše běží na portu `8080`.

## Konfigurace (.env)

| Proměnná            | Popis                                      |
|---------------------|--------------------------------------------|
| `DATABASE_URL`      | PostgreSQL connection string               |
| `ANTHROPIC_API_KEY` | API klíč pro Claude                        |
| `JWT_SECRET`        | Tajný klíč pro JWT tokeny (min 32 znaků)   |
| `JWT_EXPIRY_HOURS`  | Platnost tokenu (default 720 = 30 dní)     |
| `BASE_URL`          | Veřejná URL aplikace                       |
| `VAPID_PUBLIC_KEY`  | VAPID klíč pro push notifikace (volitelné) |
| `VAPID_PRIVATE_KEY` | VAPID privátní klíč (volitelné)            |
| `PUSH_NOTIFY_HOUR`  | Hodina pro připomínku večeře (default 20)  |

## Licence

Apache License 2.0 — viz [LICENSE](LICENSE).
