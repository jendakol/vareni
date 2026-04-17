# Vareni

Self-hosted aplikace pro správu receptů, plánování jídel a zaznamenávání, co jsme jedli.
Používá Claude API jako AI jádro.

## Funkcionalita

- **Recepty** — přidávání z textu, fotek (i více najednou) nebo URL. AI extrahuje ingredience, postup a metadata.
  Vyhledávání přes názvy, ingredience i tagy. Emoji, sdílení přes veřejný odkaz.
- **Discovery** — automatické objevování nových receptů ze 14 kurátorských webů (CZ/DE/SK/EN).
  AI hodnocení relevance, embedding deduplikace, respektování dietních omezení.
  Headless Chromium pro SPA weby se stealth evasions.
- **Plán** — navrhování jídel na x dní s ohledem na dietní omezení a historii. Potvrzování a mazání návrhů.
- **Log** — zaznamenávání co kdo jedl, per-user (každý zvlášť nebo oba). Editace a mazání záznamů, navigace po dnech.
- **Chat** — úprava receptu přes konverzaci s AI.
- **Ingestion** — automatické rozpoznání receptu z textu, fotek i webových stránek.

## Spuštění (Docker Compose)

Stačí Docker a API klíč k [Anthropic Claude](https://console.anthropic.com/).

### 1. Konfigurace

```bash
cp .env.example .env
```

V `.env` nastavte:
- `ANTHROPIC_API_KEY` — API klíč z Anthropic Console
- `JWT_SECRET` — náhodný řetězec, min 32 znaků (např. `openssl rand -hex 32`)

Ostatní hodnoty mají rozumné defaulty. Kompletní přehled viz [Konfigurace](#konfigurace).

### 2. Start

```bash
docker compose up -d
```

Aplikace běží na **http://localhost:8080**. Databázové migrace proběhnou automaticky při prvním startu.

### 3. Objevování receptů (volitelné)

Funkce discovery automaticky hledá nové recepty ze 14 webů, hodnotí je AI a filtruje duplicity
přes vektorové embeddingy. Vyžaduje stažení ONNX modelu (~86 MB):

```bash
mkdir -p models/all-MiniLM-L6-v2
cd models/all-MiniLM-L6-v2
wget https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx
wget https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json
```

Pak v `docker-compose.yml` odkomentujte řádky s `models` volume a `EMBEDDING_MODEL_DIR` a restartujte:

```bash
docker compose up -d
```

Bez modelu aplikace funguje normálně — jen funkce discovery vrací 503.

## Spuštění bez Docker Compose

Pokud nechcete použít Compose (např. máte vlastní PostgreSQL), můžete aplikaci spustit přímo:

```bash
docker build -t vareni .

docker run -d \
  --name vareni \
  -p 8080:8080 \
  -v vareni-uploads:/app/uploads \
  --env-file .env \
  vareni
```

V `.env` musí `DATABASE_URL` ukazovat na PostgreSQL s rozšířením
[pgvector](https://github.com/pgvector/pgvector). Pro discovery přidejte volume s modelem:

```bash
docker run -d \
  --name vareni \
  -p 8080:8080 \
  -v vareni-uploads:/app/uploads \
  -v ./models/all-MiniLM-L6-v2:/app/models/all-MiniLM-L6-v2:ro \
  -e EMBEDDING_MODEL_DIR=/app/models/all-MiniLM-L6-v2 \
  --env-file .env \
  vareni
```

## Konfigurace

Nastavení přes proměnné prostředí (soubor `.env`). Docker Compose nastaví `DATABASE_URL` automaticky.

| Proměnná              | Popis                                    | Default / povinné  |
|-----------------------|------------------------------------------|---------------------|
| `DATABASE_URL`        | PostgreSQL connection string             | povinné*            |
| `ANTHROPIC_API_KEY`   | API klíč pro Claude                      | povinné             |
| `JWT_SECRET`          | Tajný klíč pro JWT tokeny (min 32 znaků) | povinné             |
| `JWT_EXPIRY_HOURS`    | Platnost tokenu v hodinách               | `720` (30 dní)      |
| `BASE_URL`            | Veřejná URL aplikace                     | `http://localhost:8080` |
| `VAPID_PUBLIC_KEY`    | VAPID klíč pro push notifikace           | volitelné           |
| `VAPID_PRIVATE_KEY`   | VAPID privátní klíč                      | volitelné           |
| `PUSH_NOTIFY_HOUR`    | Hodina pro připomínku večeře             | `20`                |
| `EMBEDDING_MODEL_DIR` | Cesta k ONNX embedding modelu            | volitelné           |
| `DISCOVERY_ENABLED`   | Povolení discovery                       | `true`              |

\* Docker Compose nastavuje `DATABASE_URL` automaticky — nemusíte ho vyplňovat v `.env`.

## Vývoj

Pro přispívání do kódu nebo lokální vývoj bez Dockeru.

### Požadavky

- Rust (stable)
- Node.js 18+
- PostgreSQL 18 s rozšířením pgvector

### Databáze (jen PostgreSQL)

```bash
docker compose up -d postgres
```

### Backend

```bash
cd backend
cargo run
```

Server běží na `http://localhost:8080`. Migrace se spustí automaticky při startu.

### Frontend

```bash
cd frontend
npm install
npm run dev
```

Vite dev server běží na `http://localhost:5173` s proxy na backend.

### Produkční build frontendu

```bash
cd frontend
npm run build
```

Výsledek je v `frontend/dist/`, backend ho servuje přímo.

## Tech stack

| Vrstva     | Technologie                          |
|------------|--------------------------------------|
| Backend    | Rust / Axum                          |
| Databáze   | PostgreSQL 18 + pgvector             |
| Frontend   | Vue 3 + Vite + Tailwind CSS 4        |
| AI         | Anthropic Claude API                 |
| Embeddingy | ONNX Runtime + all-MiniLM-L6-v2     |
| Scraping   | reqwest + headless Chromium (stealth)|
| DB přístup | sqlx (async)                         |

## Licence

Apache License 2.0 — viz [LICENSE](LICENSE).
