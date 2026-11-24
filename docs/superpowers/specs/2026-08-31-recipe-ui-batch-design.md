# Recipe UI Batch — Design Spec

**Date:** 2026-08-31
**Status:** Draft — revised after adversarial review

## Goal

Six independent improvements to the recipe list and detail, agreed in brainstorming:

1. Source link on a recipe — storable, editable, clearly visible in the detail.
2. Move recipe discovery off the list page so it stops competing with search.
3. Browse all recipes (today only the first 20 are reachable).
4. Emphasise tested recipes — sort them first, mark them with a border instead of an icon.
5. Slimmer recipe cards — less text, bigger text, bigger emoji, no serving count.
6. Bug: browser Back from recipe edit lands on the list instead of the recipe detail.

Sections 0–6 below map 1:1 onto these, with section 0 holding findings that several of them depend on.

## Section 0 — Cross-cutting query fixes (prerequisite)

Three defects in `db::recipes::list` (`backend/src/db/recipes.rs`) block sections 3 and 4. They are fixed first, in one pass.

**0a. `SELECT DISTINCT` breaks search combined with sorting.**
The `q` branch selects `DISTINCT` but orders by expressions absent from the select list (`mp.last_date` for `least_cooked`, the prep+cook sum for `prep_time`). Postgres rejects that with *"for SELECT DISTINCT, ORDER BY expressions must appear in select list"*, so *Hledat* + *Nejrychlejší* / *Dlouho nevařené* returns 500. Reproduced on `postgres:18` with both queries verbatim; the fix below runs clean. No test covers `sort` at all, which is why it went unnoticed.

`DISTINCT` is unnecessary: the `WHERE` clause matches tags and ingredients through `EXISTS` subqueries, not joins, and the only join (`mp`) is pre-grouped by `recipe_id`, so no row multiplication is possible. **Fix: drop `DISTINCT`.** This also unblocks adding any ORDER BY expression later.

**0b. No stable tiebreaker.**
`ORDER BY r.updated_at DESC` is not a unique key, so the row order within a group of equal timestamps is undefined and may differ between two executions of the same query. With `LIMIT`/`OFFSET` that silently duplicates or skips rows between pages — fatal for the infinite scroll in section 3. (There is no `updated_at` trigger on `recipes` — `db::recipes::update` sets it explicitly — so migration 004's bulk `UPDATE` did *not* equalise timestamps. Ties are merely possible, not guaranteed; the fix is required either way.) **Fix: `, r.id DESC` as the final sort key in every branch.**

**0c. Order/join logic is triplicated.**
The same `{join}` and `{order}` match arms are repeated in all three branches (`tag`, `q`, plain). Sections 0a/0b/4 all touch them. **Fix: build `join` and `order` once, above the branch, and interpolate.**

Resulting order clause (single definition):

```rust
// tested first (Section 4), then the chosen criterion, then a stable tiebreaker
let order = format!(
    "ORDER BY (r.status = 'tested') DESC, {criterion}, r.id DESC",
    criterion = match sort {
        "least_cooked" => "mp.last_date ASC NULLS FIRST, r.title ASC",
        "prep_time" => &format!("{prep_time_order}, r.title ASC"),
        _ => "r.updated_at DESC",
    },
);
```

## Section 1 — Source link

### Diagnosis

The clickable link already exists in the detail (`RecipeDetailPage.vue:79`); it is almost always empty because the URL never reaches the server:

- `RecipeNewPage.vue:112` sets `source_url` on the preview object, then `RecipeForm.handleSubmit` (`RecipeForm.vue:601`) deliberately strips `source_type` and `source_url` from the payload.
- `UpdateRecipeRequest` (`backend/src/models.rs`) has no `source_url`, so it cannot be filled in later either.
- `POST /ingest` never inserts; it only parses and returns a preview.

Consequently every form-created recipe is stored as `source_type='manual'`, `source_url=NULL`. Only discovered recipes carry a URL (`db::recipes::create_discovered` binds it).

### Recovery for existing recipes

**Not possible automatically.** The URL never reached the backend, and nothing in the row distinguishes a web import from a hand-typed recipe — every one of them says `manual`. There is no ingest/audit table to mine.

Replacement: when a recipe has no `source_url`, the detail shows a low-key **„+ přidat odkaz"** affordance that opens the edit form focused on the new field, so links get filled in as the user meets the recipes.

*(Not verified against production: the `DATABASE_URL` credentials in `.env` fail authentication against production. The conclusion follows from the code paths, which admit no other writer. Confirmation query, if wanted: `SELECT source_type, status, count(*), count(source_url) FROM recipes GROUP BY 1,2;`)*

### Clear-vs-keep semantics (applies to three columns)

`db::recipes::update` uses `COALESCE($n, col)` for every optional column, and `RecipeForm.handleSubmit` sends `field || null` for empty inputs. The two combine into a silent no-op: **an emptied field keeps its old value**, and the user is shown a success toast. `source_url` would inherit the same defect, so the fix is specified once and applied to every text column the form can clear.

Backend — for `description`, `emoji` and `source_url`:

```sql
col = CASE WHEN $n IS NULL THEN col
           WHEN $n = ''    THEN NULL
           ELSE $n END
```

Absent (JSON `null`) means keep — required so that a section-only edit does not wipe a discovered recipe's URL. Empty string means clear.

Frontend — `handleSubmit` sends these three fields **verbatim as strings**, never `|| null`. This is the half that actually reaches the user: with the backend fixed and the frontend still coercing `''` to `null`, clearing stays impossible through the UI while every backend test passes. Both halves ship together, and the checklist below includes a case that drives the form rather than the API.

`servings` is deliberately excluded: it is an `Option<i32>`, so absent and cleared are the same value on the wire. Expressing "clear" would need `Option<Option<i32>>` and custom deserialisation — a different, larger change. Noted in *Out of scope*.

### Changes

**Backend**

- `UpdateRecipeRequest` gains `pub source_url: Option<String>`.
- `db::recipes::update` adopts the `CASE` form above for `description`, `emoji` and `source_url`.
- **URL validation is one shared function**, applied on every path that writes `source_url`: `CreateRecipeRequest` (currently unvalidated), `UpdateRecipeRequest`, and `db::recipes::create_discovered`, which today binds a URL harvested from the open internet straight into a column the detail renders as `<a :href>`. A scraped URL is http(s) by construction today, but the allowlist argument only holds if there is no ungated writer. Reject any scheme other than `http`/`https`; the two request paths return `400 Bad Request`, the discovery path skips the recipe. Empty string is allowed — it means clear.
- `source_type` is left alone when a user types a link onto a hand-written recipe — the column has `CHECK (source_type IN ('manual','photo','url'))` and describes how the recipe was captured, not whether a reference exists. The web-import path keeps sending `'url'`, which the form will now forward. No migration is needed anywhere in this section.

**Frontend**

- `RecipeForm`: `FormState` and `fromInitial` gain **both** `source_url` and `source_type` — `source_type` is in neither today, so adding only the URL would keep web imports landing as `source_type='manual'`. A „Odkaz na zdroj" input renders under Popis (`source_type` stays hidden, it is not a user-facing concept). `handleSubmit` sends both, replacing the comment at `RecipeForm.vue:599-601` that documented the old deliberate omission. `source_type` only matters on the create path — `UpdateRecipeRequest` has no such field, so the edit path drops it harmlessly.
- `RecipeDetailPage`: the source moves out of the times row into its own line under the title — `🔗 Zdroj: apetitonline.cz ↗`, styled as a link. Instagram keeps its friendly label. When `source_url` is empty, that line renders „+ přidat odkaz" instead, which enters edit mode.
- `editData` gains `source_url` so the edit form is populated.

### Security

Users now type arbitrary URLs into an attribute rendered as `<a :href>`, so a stored `javascript:` URL would execute on click. Two mitigations, both required:

- the shared server-side scheme allowlist above, on all three write paths, so the payload never persists;
- `rel="noopener noreferrer"` on the detail link (`RecipeDetailPage.vue:79`), which uses `target="_blank"` without it. The card needs nothing: it renders the source host as a `<span>`, and its only `<a>` is the internal `/recipes/:id` link.

Run the `sage:security-awareness` skill before implementing this section.

## Section 2 — Move discovery to its own page

Discovery moves from `/recipes` to a new route `/recipes/discover` (`DiscoverPage.vue`). The list page header then holds two buttons: `+ Nový recept` and `✨ Objevit`.

Moved from `RecipeListPage.vue`: `discoverPrompt`, `discovering`, `discoverAbort`, `handleDiscover`, `cancelDiscover`, the `discover-progress` toast id and the skip/error toast wording.

**Not moved verbatim — the page must clean up after itself.** `handleDiscover` raises a toast with `timeout: false` (`RecipeListPage.vue:153`) and holds the only `AbortController` for the request. Once discovery lives on a route the user can navigate away from, `onBeforeUnmount` must abort the in-flight request and dismiss `discover-progress`; otherwise a sticky toast follows the user around the app with no visible way to cancel the run behind it.

On success the page redirects to `/recipes?tab=discovered`. **This depends on the tab sync in section 3** — under `KeepAlive` the cached list page never re-runs `setup()`, so without it the redirect would change the address bar and nothing else.

Cancel and failure keep the user on the discover page with the prompt intact.

`discoveredCount` (the tab badge) stays on the list page and refreshes on mount and after status changes.

What is left on `/recipes`: one input, „Hledat recepty…". The wording confusion is thereby gone — nothing else on the page takes free text.

## Section 3 — Infinite scroll with restored position

Chosen over pagination explicitly: Back from a recipe must return to the exact scroll offset, Instagram-style.

**Loading.** An `IntersectionObserver` sentinel below the last card loads the next page and appends. `useRecipeStore.fetch` gains an `append` flag: `false` replaces `recipes` and resets `page`, `true` pushes. A single in-flight guard prevents double firing.

The store needs a **`loadingMore` flag separate from `loading`**. `RecipeListPage.vue:53-54` renders `v-if="store.loading"` / `v-else-if="store.error"` in place of the whole list, and `fetch` sets `loading = true` unconditionally — so as written, every append would blank the list mid-scroll, and a failed page 3 would replace 60 loaded cards with an error box. Appends set `loadingMore`; the loading, error and empty-state branches apply only while `recipes` is empty. An append failure keeps the loaded items and offers a retry in the footer.

**Termination needs two conditions, not one.** `hasMore = recipes.length < total` is not sufficient: `total` comes from a separate `COUNT(*)` and the item-level patching below never updates it, so triaging the Discovered tab drifts the two apart and the sentinel keeps firing at pages past the end — a spinner that never resolves. Therefore:

- a page returning fewer than `per_page` items marks the list exhausted, regardless of `total`;
- removing an item locally decrements `total`.

The short-page check is authoritative; the count is a hint.

**Position restore.** `App.vue` puts `KeepAlive` *inside* `RouterView` through the v-slot API — wrapping `<router-view>` in `<KeepAlive>` is the Vue 2 shape and silently caches nothing in Vue Router 4:

```vue
<router-view v-slot="{ Component }">
  <KeepAlive :include="['RecipeListPage']">
    <component :is="Component" />
  </KeepAlive>
</router-view>
```

The page needs `defineOptions({ name: 'RecipeListPage' })` for `include` to match. The router gets:

```ts
scrollBehavior(to, from, savedPosition) {
  return savedPosition ?? { top: 0 }
}
```

This works because the layout scrolls the window — `App.vue` has no inner overflow container. KeepAlive restores the DOM with every loaded batch already in it, so the saved offset is meaningful; without it, the remounted page would be 20 cards tall and the offset would clamp.

**Tab state must survive caching.** `activeTab` is a `ref` initialised from `route.query.tab` once in `setup()` (`RecipeListPage.vue:87-88`), with no watcher on the route. Under `KeepAlive`, `setup()` does not re-run, so a later navigation to `/recipes?tab=discovered` leaves the cached tab untouched and the URL then describes a screen the user is not looking at. **Fix: re-read `route.query.tab` in `onActivated`** and switch tabs when it differs.

**Mutations patch the cached list; they do not refetch it.**

An earlier draft used a `dirty` flag that refetched from page 1 after create/edit/delete. That contradicts the point of the section: the most common path of all — open a recipe from deep in the list, edit it, press Back — would land the user at offset 4000 in a freshly loaded 20-card DOM. It also omitted status changes made from the *detail* page, so marking a recipe „Vyzkoušeno" and returning would show the old border in the old position, the exact opposite of what section 4 promises.

One rule instead: **every mutation patches the store entry it affects.**

| Mutation | Effect on the cached list |
|---|---|
| status change from a card (save / reject / restore) | patch or remove that item; decrement `total` on removal |
| status change from the detail („Vyzkoušeno") | patch that item on return |
| edit | patch that item from the response body |
| delete | remove that item; decrement `total` |
| create | the only case that refetches from page 1 — the new recipe's position is unknown |

**Accepted tradeoff, stated so it is not a surprise:** a patched item keeps its *position* until the next full load. Marking a recipe tested updates its border immediately but does not float it to the top, even though tested-first sorting would place it there. Re-sorting in place would move a card out from under the user's thumb, and a jump to the top is the one thing this section exists to prevent. The order corrects itself on the next filter change or reload.

**Depends on section 0b** — without a stable total order, appending pages duplicates and skips rows.

## Section 4 — Emphasise tested recipes

**Sorting.** `(r.status = 'tested') DESC` becomes the first ORDER BY key in *all* sort modes (user's explicit choice), implemented in SQL as shown in section 0c. Doing it client-side is not an option: with paging, the browser only ever sees one batch. Consequence to accept: under *Nejrychlejší*, an untested 10-minute recipe sits below every tested one.

**Borders.** `RecipeCard.cardClass`:

| Status | Border |
|---|---|
| `tested` | solid green (`border-2 border-green-500 bg-white`) |
| `saved` | solid orange (`border-2 border-orange-400 bg-white`) |
| `discovered` | dashed blue (`border-2 border-dashed border-blue-300 bg-blue-50`) |
| `rejected`, `rejected_similar` | red, unchanged |

Green currently means *discovered*; recolouring discovered to blue frees green for "verified", which is the whole point of the change. The `✓` / `?` glyphs after the title are removed — the border replaces them. The `title` attributes („Vyzkoušeno" / „Ještě nevyzkoušeno") move onto the card so the meaning stays reachable to a screen reader.

*Known tradeoff, chosen deliberately over a weight/fill-based variant:* tested and saved then differ by hue alone — green and orange, same weight, same white background. That pair is the hardest to separate in greyscale, in bright light and for red-green colour deficiency, and the `title` fallback is a hover state a phone cannot show. The meaning of green also inverts for anyone already used to it. If the distinction does not read on a phone in the kitchen, the cheap remedy is a green tint on the tested card plus a thinner neutral border on saved — a class change, no structural work.

The detail page uses the same palette, which means swapping its two status chips: „Vyzkoušeno" turns green (today `bg-blue-100 text-blue-700`) and „Objevený (n %)" turns blue (today `bg-green-100 text-green-700`). Left as they are, the detail would say the opposite of the list.

## Section 5 — Slimmer cards

`RecipeCard.vue`, saved/tested cards:

- emoji roughly doubles (`text-3xl`), rendered as a leading block rather than an inline prefix;
- title grows to `text-xl`;
- description removed entirely;
- serving count removed;
- total time is the only remaining metadata line.

Discovered cards keep the score badge, the source host and the Uložit / Odmítnout / Odmítnout podobné buttons — without them the tab cannot be triaged. They inherit the bigger emoji and title and lose the description and servings on the same terms.

**Known consequence:** search matches `r.description` (`db/recipes.rs:283`), so a query can return a card with nothing on it explaining the match — the matching text is exactly what was removed. Accepted; title, ingredients and tags cover most real searches. If it becomes annoying, the narrow fix is a one-line matched snippet rendered *only while a search is active*, not bringing the description back.

## Section 6 — Back from edit

`editing` is a local `ref`, so entering edit pushes no history entry and Back leaves the detail route altogether.

Edit mode becomes `?edit=1` on the detail route: the pencil button calls `router.push({ query: { edit: '1' } })`, and `editing` is `computed(() => route.query.edit === '1')`. Leaving edit — the „← Zpět" button and a successful save — uses `router.replace({ query: {} })`, not `router.back()`: on a reload or a deep link straight into `?edit=1` there is no previous entry and `back()` would leave the app. `replace` always lands on the detail, and the browser Back button still works because entering edit pushed an entry.

**Unsaved-changes guard — required, not optional.** Today Back out of an edit is broken in a way that merely annoys. After this change it becomes a smooth, single-gesture way to discard a half-finished edit: no confirmation, no draft, nothing. On a phone the edge-swipe back gesture is easy to trigger by accident. The section is not done without a guard.

The detail that is easy to get wrong: `?edit=1` → `{}` is a **query-only change on the same route record**, so `onBeforeRouteLeave` does *not* fire — the guard must be `onBeforeRouteUpdate`. A guard written on the wrong hook compiles, reads as correct, and never runs. Register both: `onBeforeRouteUpdate` for leaving edit mode, `onBeforeRouteLeave` for navigating away from the detail entirely while still editing.

The guard prompts only when the form is dirty (`RecipeForm` exposes a `dirty` computed comparing current state with `fromInitial`), and a successful save sets a `bypassGuard` flag before `router.replace`, so saving never asks.

Query param plus `v-if` remount, not a child route: `RecipeForm` re-initialises only when `props.initial?.id` changes, and `editData` does not include `id`, so a form kept mounted across the transition would never reload its data.

## Testing

**Backend** (`backend/tests/recipes_test.rs`, existing harness):

- tested-before-saved holds for each of `recent`, `prep_time`, `least_cooked`;
- `q` combined with each sort returns 200 — the regression from 0a;
- paging across a batch boundary with identical `updated_at` yields every recipe exactly once — the regression from 0b;
- `PUT` without `source_url` preserves the stored value; with a URL sets it; with `""` clears it;
- the same three cases for `description` and `emoji`;
- `POST`, `PUT` and the discovery insert all reject `javascript:alert(1)` and persist nothing.

**Frontend** — no test framework in `frontend/package.json`, so `npm run build` (`vue-tsc`) plus a manual checklist carried into the implementation plan:

- **edit a recipe, change only the title, save → `source_url` and `description` are unchanged in the DB.** The backend contract can be green while the form still coerces `''` to `null`; this is the case that catches it;
- clear the source link in the form and save → it is gone after a reload. Same for the description;
- scroll deep into the list, open a recipe, press Back → same offset, same loaded batches;
- scroll deep, open a recipe, edit and save it, press Back → same offset, card shows the new data;
- open a recipe, mark „Vyzkoušeno", press Back → the card shows the green border and the list does not jump;
- start editing, type into a field, press Back → a confirmation appears; confirm discards, cancel stays in the form;
- save from the edit form → no confirmation, lands on the detail;
- triage the Discovered tab down to empty → the list ends cleanly, no permanent „Načítám…";
- change tab / sort / search → the list resets to the top;
- start a discovery, navigate away mid-run → the sticky toast is gone and the request is aborted;
- a discovery finishes → the list opens on the Objevené tab, matching the URL;
- a failed page-3 append keeps the loaded cards and offers a retry;
- import from web → the detail shows the source link.

## Out of scope

- **`servings` cannot be cleared** through the form, for the same absent-vs-empty reason as the text columns, but the fix differs: `Option<i32>` cannot express it, so the request type would need `Option<Option<i32>>` and custom deserialisation. Separate change.
- Link rot and tracking parameters on stored source URLs — a link that later 404s, turns into a paywall or carries `utm_*` is shown as-is, and the card displays only the host.
- Undo / soft delete. Deleting a recipe is a hard cascade delete behind a confirm dialog; unchanged here. (Editing is already safe: `db::recipes::update` runs in a transaction — `recipes.rs:372`/`556` — so a partly applied section rewrite cannot happen.)
- The `list()` branch structure itself (tag / q / plain) stays as is; only the shared order and join move out.
