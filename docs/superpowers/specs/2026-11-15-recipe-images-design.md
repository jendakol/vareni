# Recipe images: cover photo + source screenshots

## Why

- **Cover photo**: makes the recipe list visually scannable; must be easy to take with a phone.
- **Source screenshots**: assurance and comparison — to check how exactly the original source described the recipe (the AI parse can be wrong or the source can disappear).
- Source link stays as is (single `source_url`, no change).

All three are optional.

## Goals

- Optional cover photo per recipe, uploadable from gallery or phone camera, shown in the recipe list.
- Optional, any number of source screenshots per recipe, viewable fullscreen with zoom.
- Photo-import originals are kept automatically as screenshots.
- URL import (and discovery) pre-fills the cover from the page's `og:image`.

## Non-goals

- Automatic screenshots of source web pages (considered and rejected — see Approaches).
- Multiple source links.
- Reordering screenshots (ordered by upload time).
- Image editing / cropping in the app.
- HEIC decoding (iOS Safari converts to JPEG on upload).

## Anti-goals

- Leaking GPS location from phone photos (EXIF) — especially via publicly shared recipes.
- Screenshots reachable without login (they are third-party content, may contain other people's names/comments).
- A list that loads multi-MB originals on mobile.

## Decisions

### Screenshots — manual only (approach A)

Screenshots are added manually on the recipe detail, plus photo-import images are kept automatically.
Rejected: automatic full-page Chromium screenshot on URL import — cookie banners, Instagram login walls, 15 000 px tall unreadable images, slower import, and Chromium would have to run for every URL import. Could be revisited later.

### Storage & processing — server-side with `image` crate (approach 1)

- Files on disk in the existing `uploads` volume (`UPLOAD_DIR`).
- Backend accepts the original, applies EXIF orientation, writes two JPEG variants and drops all metadata (re-encoding does that):
  - cover: `full` longest side ~1600 px, q≈82;
  - screenshot: `full` capped by **width** (~1440 px) with height up to 12 000 px, q≈90 — tall phone and scrolling screenshots must stay readable, that is their whole purpose;
  - both: `thumb` ~400 px (screenshot thumb is a top crop).
- Accepted input: JPEG, PNG, WebP; max 20 MB per file; detect format from content, not from the client's content type.
- Frontend downsizes before upload purely to speed up mobile uploads; the server always re-processes regardless. Covers only (to ~2500 px) and only when the file is > ~3 MB; screenshots go as-is. Use `createImageBitmap` with resize options; if it fails or yields an empty image, upload the original (iOS canvas limits can silently produce black images).
- Decode guarded by `image::Limits` (max 12 000 px per side, allocation cap), header dimensions checked before full decode, and decode/resize behind a semaphore of 2 to keep a small box from OOM.
- Rejected: client-only resizing (server would trust client; photo import already sends originals to server); Postgres `bytea` (bloats DB/backups, volume already exists).

### Data model

New migration:

```sql
CREATE TABLE recipe_images (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  recipe_id UUID NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('cover', 'screenshot')),
  width INTEGER NOT NULL,   -- of the full variant
  height INTEGER NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX recipe_images_one_cover ON recipe_images (recipe_id) WHERE kind = 'cover';
CREATE INDEX recipe_images_recipe ON recipe_images (recipe_id, created_at);
ALTER TABLE recipes DROP COLUMN cover_image_path;  -- never written by any code
```

Files: `{UPLOAD_DIR}/recipes/{recipe_id}/{image_id}_full.jpg` and `_thumb.jpg` (path derived from IDs, not stored).
Uploading a new cover replaces the old one (row + files) in one transaction with a row lock; a concurrent replace gets a clean conflict error instead of hitting the unique index.
Deleting a recipe deletes its directory (DB rows go via cascade). `db::recipes::delete` is the only recipe delete path; rejected/discovered recipes are status changes and keep their images (they can be restored). File deletion is best-effort after the DB commit; failures are logged, not surfaced.

*My call (write order)*: write both variants to temp names in the recipe directory, insert the row, then rename into place; on any failure remove what was written. A crash in between can leak a file — accepted and logged, no orphan sweep.

### Serving — authenticated

- Remove the unauthenticated `/uploads` `ServeDir` (nothing uses it today).
- `GET /api/recipes/{id}/images/{image_id}/{full|thumb}` — requires session (cookie auth works for `<img src>`), same access rule as reading the recipe. Today that rule is "any logged-in user" (recipes are shared by the household; routes don't check `owner_id`), so the check is: logged in, and the image row belongs to that recipe. Long `Cache-Control: private, max-age` (image IDs are immutable; a replaced cover gets a new ID).
- `GET /api/public/recipes/{slug}/cover/{full|thumb}` — cover only, only while the recipe is public. Screenshots are never public. `Cache-Control: no-cache` + ETag (image ID), so a replaced cover or an unshare takes effect immediately.
- The public recipe endpoint today returns the full `RecipeDetail` (incl. `owner_id`). It gets a dedicated public DTO without `owner_id` and without `screenshots`.

### API

- `POST /api/recipes/{id}/images` — multipart, fields `kind` (`cover`|`screenshot`) and one or more `image`. Returns the created image records.
- `DELETE /api/recipes/{id}/images/{image_id}`.
- `Recipe` response gains `cover: {id, width, height} | null` and `screenshots: [{id, width, height}]` (detail); list items need only `cover` — fetch it in the list query (LEFT JOIN on the partial unique index), no N+1.
- Permission for upload/delete: same as recipe update (any logged-in user, see above).
- No limit on screenshot count per recipe (household app, trusted users). Decode is guarded by `image::Limits` (max ~50 MP and an allocation cap) against decompression bombs, on top of the 20 MB file cap.
- PNG with transparency is flattened onto white; animated WebP uses the first frame.

### og:image cover

- URL ingest extracts `og:image` (fallback: JSON-LD `image`) and returns it as `cover_image_url` in the ingest preview; the preview shows it.
- `CreateRecipeRequest` gains optional `cover_image_url`. On create the backend downloads it (http/https only, reject private/loopback IPs, 10 s timeout, 20 MB cap, must decode as an image), processes it like an upload and stores it as the cover. Download failure does not fail the create — recipe is saved without cover, warning logged.
- Discovery (user's decision: keep it simple, a bad cover is removed with ✕): `og:image` is extracted from the HTML discovery already fetches (no extra request for the page). After `create_discovered` succeeds, the caller downloads and stores the cover best-effort, using the same shared "fetch → process → store cover" service function as the URL-ingest create. The DB function stays DB-only.
- SSRF protection (the riskiest piece): a dedicated `reqwest` client with a custom DNS resolver that rejects loopback, private, link-local, CGNAT, unique-local and unspecified addresses (v4 and v6, including v4-mapped). Because the check runs in the resolver, it applies to every connection, including each redirect hop, and closes the DNS-rebinding gap of a separate pre-check. Max 3 redirects, http/https only, no cookies, response streamed with a 20 MB cap. Today `validation::check_source_url` checks the scheme only, so this is new code.

### Photo import → screenshots

The frontend keeps the `File` objects chosen in the photo import tab; after the recipe is created it uploads them as `screenshot`s. The photo tab's `accept` is narrowed to the same JPEG/PNG/WebP list, with an explicit message for HEIC. *My call*: the first photo does **not** become the cover — a photo of a cookbook page or a screen is a poor cover; the user can add one on the detail. No server-side temporary storage. If that upload fails, the recipe stays saved and the detail shows a persistent banner "Fotky zdroje nebyly uloženy – Zkusit znovu" (while the tab still holds the files) instead of a vanishing toast. Accepted risk: if the mobile browser discards the tab mid-flow, the originals are not kept.

### UI

**Recipe list — "Moje" tab**: 2-column grid of tiles (mockup C): square image (cover `thumb`, or emoji on a light background when no cover), title (may wrap), total time.
The cover is added to `db::recipes::list` (LEFT JOIN), which serves all three tabs; its other callers (discovery dedup, meal plan) get the field and ignore it. Showing covers in the meal plan is out of scope.
**"Objevené" / "Odmítnuté" tabs**: keep current row layout with the existing badges and buttons, plus a small square thumbnail on the left (mockup A). These tabs are a triage queue where clear buttons matter more than browsing.

**Recipe detail**:

```
┌──────────────────────────┐
│     [ cover 4:3 ]        │  no cover: grey area "📷 Přidat fotku"
│           [Změnit][✕]    │
├──────────────────────────┤
│ 🍝 Carbonara             │
│ Zdroj: example.cz ↗      │
│ Původní zdroj:           │
│ [▢][▢][▢] [+ screenshot] │  tap = fullscreen, pinch-zoom, swipe
│ Ingredience …            │
```

- Uploads happen immediately on file selection, no preview/confirm step and no edit mode (user's decision, after considering the mis-tap risk on public recipes).
- File input: `accept="image/jpeg,image/png,image/webp"` (screenshots: `multiple`), **no** `capture` attribute — mobile offers camera or gallery.
- Fullscreen viewer: prev/next swipe, native pinch-zoom, delete button (with confirm).
- Upload progress per file. A failed file stays as a tile with an error and "Zkusit znovu" until it uploads or is dismissed — a camera shot may exist nowhere else.
- Cover ✕ asks for confirmation. Recipe delete confirm says "včetně N fotek".
- Android back closes the fullscreen viewer (push a history state).

**Recipe create (URL tab)**: preview shows the og:image cover with an option to drop it.
**Public recipe page**: shows cover if present; no screenshots.

## Related fixes (same branch, separate commit)

- `generate_slug` slices the title by bytes (`&base[..40]`) and can panic on Czech multibyte characters — truncate by chars.
- Public slug suffix is only 4 random digits (guessable from the title) — use a longer random suffix (e.g. 8 base36 chars). Existing slugs stay valid.

## Operations

- README: add the `uploads` volume to the backup/restore procedure next to `pg_dump`; drop the "`/uploads` je bez autentizace" note.
- No orphan sweep (see write order). Export of recipes + images is out of scope.

## Constraints

- Rust/axum backend, sqlx migrations in `backend/migrations`, Vue 3 + Tailwind frontend — follow existing patterns.
- Single Docker image, `uploads` volume already mounted at `/app/uploads`.
- Image decoding of a 12 MP photo is ~50 MB RAM; run decode/resize in `spawn_blocking`.

## Left to the builder

- Exact sizes/JPEG quality (start at 1600/400 px, q≈82).
- Lightbox implementation (small library vs. hand-written).
- Multipart body size limit config in axum (must allow ≥ 20 MB per file, several files).
- Concrete error codes/messages.
