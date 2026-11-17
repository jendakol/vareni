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
- `OIDC_ISSUER_URL`, `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET`, `OIDC_REDIRECT_URI` — přihlášení přes Keycloak, viz [Přihlášení (OIDC)](#přihlášení-oidc)

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

## Příklady nasazení

### docker-compose.yml

Repozitářový `docker-compose.yml` staví image z lokálních zdrojů. Varianta s povoleným discovery
(model stažený podle kroku 3) a s hodnotami přímo v souboru místo `.env`:

```yaml
services:
  postgres:
    image: pgvector/pgvector:pg18
    environment:
      POSTGRES_USER: cooking
      POSTGRES_PASSWORD: cooking   # změňte, pokud je DB dostupná mimo Compose síť
      POSTGRES_DB: cookingapp
    volumes:
      - pgdata:/var/lib/postgresql/data
    restart: unless-stopped

  app:
    build: .
    ports:
      - "8080:8080"
    environment:
      DATABASE_URL: postgresql://cooking:cooking@postgres:5432/cookingapp
      ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY}
      BASE_URL: https://vareni.example.com
      OIDC_ISSUER_URL: https://keycloak.example.com/realms/master
      OIDC_CLIENT_ID: vareni
      OIDC_CLIENT_SECRET: ${OIDC_CLIENT_SECRET}
      OIDC_REDIRECT_URI: https://vareni.example.com/api/auth/callback
      EMBEDDING_MODEL_DIR: /app/models/all-MiniLM-L6-v2
    volumes:
      - uploads:/app/uploads
      - ./models/all-MiniLM-L6-v2:/app/models/all-MiniLM-L6-v2:ro
    depends_on:
      - postgres
    restart: unless-stopped

volumes:
  pgdata:
  uploads:
```

`${ANTHROPIC_API_KEY}` a `${OIDC_CLIENT_SECRET}` Compose doplní z prostředí nebo ze souboru `.env`
vedle `docker-compose.yml`. Secrety do souboru nepište natvrdo.

### docker run (bez Compose)

Pokud nechcete použít Compose (např. máte vlastní PostgreSQL), sestavte image a spusťte ho přímo:

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
  -v "$PWD/models/all-MiniLM-L6-v2:/app/models/all-MiniLM-L6-v2:ro" \
  -e EMBEDDING_MODEL_DIR=/app/models/all-MiniLM-L6-v2 \
  --env-file .env \
  vareni
```

Kompletní příklad včetně vlastní databáze na sdílené síti:

```bash
docker network create vareni-net

docker run -d --name vareni-db --network vareni-net \
  -e POSTGRES_USER=cooking -e POSTGRES_PASSWORD=cooking -e POSTGRES_DB=cookingapp \
  -v vareni-pgdata:/var/lib/postgresql/data \
  pgvector/pgvector:pg18

docker run -d --name vareni --network vareni-net \
  -p 8080:8080 \
  -v vareni-uploads:/app/uploads \
  -e DATABASE_URL=postgresql://cooking:cooking@vareni-db:5432/cookingapp \
  --env-file .env \
  vareni
```

## Přihlášení (OIDC)

Přihlášení řeší Keycloak (authorization code + PKCE, server drží session v HttpOnly cookie `__Host-vareni_session`). Přihlásí se jen uživatel, jehož Keycloak `sub` je uložen v `users.external_id` **a** který má v ID tokenu roli `OIDC_REQUIRED_ROLE`. Jinak 403.

### Nastavení Keycloaku
1. Zálohujte realm (export).
2. Client `vareni`: confidential, standard flow, PKCE S256. Valid redirect URI `https://<host>/api/auth/callback`, valid post logout redirect URI `https://<host>/`.
3. Mapper typu *User Realm Role*: claim `roles` (bez teček), multivalued, *Add to ID token* zapnuto.
4. Roli `vareni` přiřaďte jen uživatelům, kteří mají mít přístup. V realmu nesmí být povolená self-registrace.
5. ID token musí být podepsaný RS256 (výchozí v Keycloaku); jiný algoritmus realmu = `invalid_token`.
6. `OIDC_ISSUER_URL` musí přesně odpovídat `iss` v tokenu (veřejné URL, ne interní hostname).

### Pořadí nasazení
1. `pg_dump` a ověření obnovy. Zálohujte i volume `uploads` (obrázky receptů), viz [Zálohy](#zálohy-a-obrázky).
2. Nastavit Keycloak a `.env`.
3. Nasadit nový image (aplikuje migraci 009; do nasazení seedu každý dostane 403).
4. Propojit uživatele: `UPDATE users SET external_id = '<sub>' WHERE name = '<jméno>';`
5. Vyzkoušet přihlášení. Sloupec `password_hash` zůstává (nullable) a smaže se až pozdější migrací.

Po nasazení jsou všichni odhlášeni. Rollback: obnovit dump, nebo `DELETE FROM _sqlx_migrations WHERE version = 9` a ruční revert (staré verze po 009 nenastartují). Stará verze navíc vyžaduje `JWT_SECRET` (min. 32 znaků) a uživatele s `password_hash`; bez něj rollback nejde.

### Nouzový přístup a správa session
```bash
docker compose exec app ./cooking-app create-session <jméno>   # vypíše hodnotu cookie
docker compose exec app ./cooking-app revoke-sessions <jméno>  # zneplatní všechny session uživatele
```
Příkazy vyžadují `DATABASE_URL`. Odebrání role v Keycloaku existující session neukončí — použijte `revoke-sessions`.

### Poznámky
- Odhlášení volá Keycloak end_session, takže ukončí SSO pro celý realm.
- `OIDC_ROLE_CLAIM` nesmí ukazovat na claim, který si uživatel může sám upravit (např. atribut profilu).
- `/metrics` je bez autentizace.
- `POST /api/log` (Home Assistant) dál používá statický Bearer `LOG_API_TOKEN`.

## Zálohy a obrázky

Obrázky receptů leží ve volume `uploads` (`/app/uploads`) a obsluhují se jen přihlášeným uživatelům, kromě obálky veřejně sdíleného receptu. Zálohujte je spolu s `pg_dump` (u `docker run` se volume jmenuje `vareni-uploads`, u Compose má prefix projektu, např. `vareni_uploads`):

```bash
docker run --rm -v vareni-uploads:/data -v $PWD:/backup alpine tar czf /backup/uploads.tgz -C /data .   # záloha
docker run --rm -v vareni-uploads:/data -v $PWD:/backup alpine tar xzf /backup/uploads.tgz -C /data     # obnova
```

Reverse proxy před aplikací musí povolit těla požadavků alespoň 100 MB (limit aplikace), např. v nginx `client_max_body_size 100m;`.

## Konfigurace

Nastavení přes proměnné prostředí (soubor `.env`). Docker Compose nastaví `DATABASE_URL` automaticky.

| Proměnná              | Popis                                    | Default / povinné  |
|-----------------------|------------------------------------------|---------------------|
| `DATABASE_URL`        | PostgreSQL connection string             | povinné*            |
| `ANTHROPIC_API_KEY`   | API klíč pro Claude                      | povinné             |
| `AI_MODEL_CHAT`       | Claude model pro chat                    | `claude-sonnet-5-5` |
| `AI_MODEL_INGEST`     | Claude model pro import receptů          | `claude-sonnet-5-5` |
| `AI_MODEL_PLAN`       | Claude model pro plán jídelníčku         | `claude-haiku-5-5`  |
| `AI_MODEL_DISCOVERY`  | Claude model pro discovery               | `claude-haiku-5-5`  |
| `OIDC_ISSUER_URL`     | Issuer Keycloaku (musí přesně odpovídat `iss`) | nutné pro login       |
| `OIDC_CLIENT_ID`      | Client ID v Keycloaku                    | nutné pro login             |
| `OIDC_CLIENT_SECRET`  | Secret confidential klienta              | nutné pro login             |
| `OIDC_REDIRECT_URI`   | `https://<host>/api/auth/callback`       | nutné pro login             |
| `OIDC_POST_LOGOUT_REDIRECT_URI` | Návrat po odhlášení            | `{BASE_URL}/`       |
| `OIDC_REQUIRED_ROLE`  | Role vyžadovaná v ID tokenu              | `vareni`            |
| `OIDC_ROLE_CLAIM`     | Claim s rolemi (tečková cesta povolena)  | `roles`             |
| `SESSION_TTL_HOURS`   | Platnost session                         | `720` (30 dní)      |
| `COOKIE_SECURE`       | Secure cookie (v produkci nelze vypnout) | `true`              |
| `ENVIRONMENT`         | `local` povolí HTTP vývoj (a dev-login při buildu s feature `debug-auth`) | `production`        |
| `BASE_URL`            | Veřejná URL aplikace                     | `http://localhost:8080` |
| `EMBEDDING_MODEL_DIR` | Cesta k ONNX embedding modelu            | volitelné           |
| `DISCOVERY_ENABLED`   | Povolení discovery                       | `true`              |
| `LOG_API_TOKEN`       | Bearer token pro `POST /api/log` (Home Assistant) | volitelné   |
| `METRICS_GAUGE_REFRESH_SECS` | Jak často se obnovuje gauge `recipes_total` | `60`        |

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
