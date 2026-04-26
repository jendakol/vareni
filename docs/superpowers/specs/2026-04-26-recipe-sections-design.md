# Recipe Sections — Design Spec

**Date:** 2026-04-26
**Status:** Draft — pending user review

## Goal

Support recipes that consist of multiple distinct parts (e.g. *těsto*, *náplň*, *drobenka*) — each with its own ingredients, steps, optional description, and time. Recipe-level total times become the sum of section times.

## Motivation

Example: https://www.vareni.cz/recepty/svestkovy-kolac-s-tvarohem/ — three logical parts. Today the AI ingester squashes them into the flat `recipe_ingredients.note` column ("těsto", "náplň", "drobenka"). That breaks rendering, search, and editing.

## Architecture

Sections become a first-class entity. Every recipe has at least one section. Single-section recipes use one anonymous section (`label = NULL`) and the UI hides section management entirely — feels like today's flat recipe.

Recipe-level `prep_time_min` / `cook_time_min` are removed from `recipes` and computed from `SUM` over the recipe's sections. API responses surface them as derived fields on the same names.

Ingredients and steps both gain a non-nullable `section_id` FK. Step `step_order` becomes per-section, not global.

## Schema (migration `005_recipe_sections.sql`)

The whole migration runs in a single transaction (Postgres allows transactional DDL). The app should be stopped or in maintenance mode during migration — the deployment is a single Docker Compose stack on the user's home host, no rolling upgrade concerns.

```sql
BEGIN;
CREATE TABLE recipe_sections (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  recipe_id UUID NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
  label TEXT,                    -- NULL = anonymous (single-section mode)
  description TEXT,
  prep_time_min INTEGER,
  cook_time_min INTEGER,
  sort_order INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_recipe_sections_recipe ON recipe_sections(recipe_id, sort_order);

ALTER TABLE recipe_ingredients
  ADD COLUMN section_id UUID REFERENCES recipe_sections(id) ON DELETE CASCADE;
ALTER TABLE recipe_steps
  ADD COLUMN section_id UUID REFERENCES recipe_sections(id) ON DELETE CASCADE;

-- Backfill: one default section per existing recipe, copy times into it
INSERT INTO recipe_sections (id, recipe_id, label, prep_time_min, cook_time_min, sort_order)
SELECT gen_random_uuid(), id, NULL, prep_time_min, cook_time_min, 0
FROM recipes;

UPDATE recipe_ingredients ri
SET section_id = s.id
FROM recipe_sections s
WHERE s.recipe_id = ri.recipe_id;

UPDATE recipe_steps rs
SET section_id = s.id
FROM recipe_sections s
WHERE s.recipe_id = rs.recipe_id;

-- Make section_id mandatory after backfill
ALTER TABLE recipe_ingredients ALTER COLUMN section_id SET NOT NULL;
ALTER TABLE recipe_steps        ALTER COLUMN section_id SET NOT NULL;

ALTER TABLE recipes DROP COLUMN prep_time_min;
ALTER TABLE recipes DROP COLUMN cook_time_min;
COMMIT;
```

**Invariant:** every recipe has ≥1 section row.

**Backfill outcome:** every existing recipe becomes a single anonymous section (`label = NULL`, `sort_order = 0`) that owns all of the recipe's existing ingredients, steps, and times. Nothing is lost; only the column location changes. The recipe-level time columns are then dropped — recipe times are derived from `SUM` over sections from this point on. UI behaviour for these recipes is identical to pre-migration (no section header rendered, times shown at recipe level).

**Cascade choice:** `ON DELETE CASCADE` on `section_id`. Deleting a section removes its ingredients and steps. The form-level UX prompts the user before delete and offers to move rows to another section — that's done by re-pointing FKs *before* deletion.

**`recipe_steps.step_order`** becomes per-section. The PK becomes `(section_id, step_order)`. The current PK `(recipe_id, step_order)` is dropped.

```sql
ALTER TABLE recipe_steps DROP CONSTRAINT recipe_steps_pkey;
ALTER TABLE recipe_steps ADD PRIMARY KEY (section_id, step_order);
```

## Backend

### Models (`backend/src/models.rs`)

```rust
pub struct RecipeSection {
    pub id: Uuid,
    pub recipe_id: Uuid,
    pub label: Option<String>,
    pub description: Option<String>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub sort_order: i32,
}

pub struct RecipeSectionWithContent {
    #[serde(flatten)]
    pub section: RecipeSection,
    pub ingredients: Vec<RecipeIngredient>,
    pub steps: Vec<RecipeStep>,
}

pub struct RecipeDetail {
    #[serde(flatten)]
    pub recipe: Recipe,            // recipe.prep_time_min / cook_time_min are derived
    pub sections: Vec<RecipeSectionWithContent>,
    pub tags: Vec<String>,
}
```

`Recipe` keeps `prep_time_min: Option<i32>` and `cook_time_min: Option<i32>` as **derived** fields. `recipes.rs` queries fill them with `SELECT SUM(...) FROM recipe_sections WHERE recipe_id = $1` (or computed in app code after fetching sections).

`RecipeIngredient` and `RecipeStep` gain `pub section_id: Uuid`.

### Inputs

```rust
pub struct SectionInput {
    pub label: Option<String>,
    pub description: Option<String>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub sort_order: i32,
    pub ingredients: Vec<IngredientInput>,
    pub steps: Vec<StepInput>,
}

pub struct CreateRecipeRequest {
    pub title: String,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub emoji: Option<String>,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub tags: Option<Vec<String>>,
    pub sections: Vec<SectionInput>,    // must contain ≥1
}
```

`UpdateRecipeRequest` mirrors create. Sections without an `id` are inserts; sections with an `id` are updates; sections present in DB but missing from payload are deletes.

**No legacy flat-input compatibility.** The frontend ships in lockstep with the backend; no third-party API consumers exist. Reject requests missing `sections` with 400. (Frontend developers: if you see this error during development, you have a stale client.)

### CRUD (`backend/src/db/recipes.rs`)

`create_recipe`:
1. Insert `recipes` row.
2. For each input section: insert `recipe_sections`, then its ingredients (resolving `ingredients` table ID) and steps with `section_id`.

`get_recipe`: load recipe + all sections (ordered by `sort_order`) + their ingredients (ordered by `sort_order`) and steps (ordered by `step_order`). Compute `Recipe.prep_time_min`/`cook_time_min` from sum.

`update_recipe`:
1. **Validate every incoming `section_id` belongs to the target recipe** — `SELECT id FROM recipe_sections WHERE recipe_id = $target AND id = ANY($incoming_ids)`. Reject with 400 if any incoming `id` is missing from that set. Prevents cross-recipe ID injection.
2. Diff sections by `id`. For each kept section: update label/desc/times/sort_order, then diff ingredients (delete-all-insert-all is fine for ingredient/step rows — they're cheap and section-scoped).
3. New sections (no `id` in payload): insert.
4. Removed sections (in DB but missing from payload): delete (CASCADE handles their ingredients/steps).

`delete_recipe`: unchanged (CASCADE handles everything).

### List endpoints

`list_recipes` returns `Recipe` summaries; needs derived times. Implement via a single query with `LEFT JOIN LATERAL (SELECT SUM(prep_time_min) AS p, SUM(cook_time_min) AS c FROM recipe_sections WHERE recipe_id = r.id) ON true` or a correlated subquery.

## AI Ingest (`backend/src/ai/ingest.rs`)

### Output schema

```json
{
  "title": "string",
  "description": "string|null",
  "servings": "number|null",
  "tags": ["string"],
  "sections": [
    {
      "label": "string|null",
      "description": "string|null",
      "prep_time_min": "number|null",
      "cook_time_min": "number|null",
      "ingredients": [{ "name": "string", "amount": "number|null", "unit": "string|null", "note": "string|null" }],
      "steps": [{ "step_order": "number", "instruction": "string" }]
    }
  ],
  "guessed_fields": ["string"]
}
```

### Prompt rules (Czech wording in actual prompt)

1. **A heading becomes a section ONLY when it has at least one ingredient OR step directly under it.** Sub-headings like "Tip", "Poznámka", "Jak podávat", "Varianty", "Podávání" are NOT sections — fold their text into the *recipe* description (or the nearest preceding section's description if it's clearly part-specific).
2. **If the source organizes ingredients or steps under sub-headings** — "Na těsto", "Náplň", "Drobenka", "For the dough", "For the filling" — emit one section per heading **provided rule 1 holds**. Use the heading text as `label`, in Czech.
3. **If the source has no part sub-headings, or only has informational sub-headings**, emit exactly one section with `label: null`.
4. **Don't invent sections.** Only split when the source explicitly groups *content* (ingredients/steps), not just prose.
5. **Conditional headings** ("Krém A nebo B", "Volitelná náplň") — emit them as a single section only if both alternatives share an ingredient list. If they have separate ingredient lists, emit separate sections. If unclear, default to one section and put the alternatives in the description.
6. **`step_order`** is per-section, starting at 1 for each section.
7. **Times:** if the source gives only one total time, leave **all** per-section times `null` and put the total in `guessed_fields` for the first section's `prep_time_min` (or `cook_time_min`). The frontend treats a recipe where only one section has a non-null time as "single total" and shows it at recipe level only, suppressing per-section display. If per-section times are explicit in the source, use them.
8. **`description`** on a section is optional; only fill if the source has an intro line for that part.
9. The recipe-level `description` is the overall recipe intro, separate from section descriptions.
10. Drop the recipe-level `prep_time_min`/`cook_time_min` from the schema entirely — caller computes from sections.
11. **`guessed_fields`**: if any section was inferred, include `"sections"`. Per-section guesses follow the existing logic.

### Parser

`ParsedRecipe` struct mirrors the schema. Maps directly to `CreateRecipeRequest`/`UpdateRecipeRequest` shape.

## Frontend

### API types (`frontend/src/api/recipes.ts`)

```ts
export interface Section {
  id: string
  label: string | null
  description: string | null
  prep_time_min: number | null
  cook_time_min: number | null
  sort_order: number
  ingredients: Ingredient[]
  steps: Step[]
}

export interface Recipe {
  // ...
  prep_time_min: number | null   // derived total
  cook_time_min: number | null   // derived total
  sections?: Section[]
  // legacy `ingredients` and `steps` arrays REMOVED
}
```

### Display

**`IngredientList.vue`** — accepts `Section[]` instead of `Ingredient[]`. Renders one `<h4>` per labeled section with optional description, then the ingredient list. For single-section recipes (one section, `label = null`), renders no header — output matches today.

**Steps rendering** (currently inline in `RecipeDetailPage.vue` and `PublicRecipePage.vue`) — extract a `StepList.vue` component that takes `Section[]` and renders steps grouped under section headers. **Display numbering is continuous** across sections (1., 2., 3., …) for the cook's reading experience. **Storage stays per-section** (`step_order` is `1..n` within each `section_id`). The edit form maps display numbers ↔ storage rows by reference (each step row carries its `(section_id, step_order)` identity in component state), never by display index, so reordering sections never causes the user to edit the wrong step.

**`RecipeDetailPage.vue` / `PublicRecipePage.vue`** — pass sections to `IngredientList` and `StepList`. Show recipe-level total times (`recipe.prep_time_min`, `recipe.cook_time_min`) as today.

**Delete-recipe confirmation** — extend the existing delete confirm to name what's about to vanish: *"Smazat recept „X"? Obsahuje N skupin, M ingrediencí, P kroků. Akce je nevratná."*

**Per-section time display in detail view** — when the recipe has >1 section but only one section has a non-null time (the LLM "single total" case from prompt rule 7), render the time at recipe level only and suppress per-section time labels. Otherwise render per-section times under each section header. Computed via `sections.filter(s => s.prep_time_min != null || s.cook_time_min != null).length`.

**`CookingMode.vue`** — when current step belongs to a labeled section, show the section label as a sub-header above the step.

### Edit form (`RecipeForm.vue`)

**Default state — single-section mode:**
- Looks like today: `Ingredience` block with rows, `Postup` block with rows, two time inputs.
- Internally backed by `sections = [{ label: null, ingredients: [...], steps: [...] }]`.

**Toggle:** checkbox/switch **"Recept má více částí"**.
- ON: section UI appears.
  - Each section gets an editable `label` input, optional `description` textarea, two time inputs (prep / cook), drag handle for reordering, **"Smazat skupinu"**.
  - The delete-section confirmation names the section's content explicitly: *"Smazat skupinu „Náplň"? Smaže se 5 ingrediencí a 2 kroky. [Přesunout obsah do skupiny ▾] [Smazat] [Zrušit]"*. The "Přesunout" path moves rows via FK update, then deletes the now-empty section.
  - **"+ Přidat skupinu"** at the bottom appends a new empty section.
  - Recipe-level time inputs disappear; per-section times take over. Recipe total is shown read-only.
  - Drag/drop ingredient rows between sections.
- OFF: collapses back to flat view. Always available — clicking it triggers a **destructive-action confirmation**: *"⚠️ Sloučit X skupin do jedné? Zachová se obsah všech skupin v pořadí — ALE NÁZVY A POPISY VŠECH SKUPIN BUDOU NENÁVRATNĚ ODSTRANĚNY. Per-section časy se sečtou do jednoho. [Sloučit a zrušit skupiny] [Zrušit]"*. The destructive button is styled red and is not the default focus. On confirm, the form merges all sections into a single anonymous section, concatenating ingredients (preserving cross-section order) and steps (renumbered 1..N), summing per-section times into the resulting single section.
- **Anonymous label invariant:** when the recipe is in single-section mode (one section, anonymous), the form prevents typing a label by hiding the label input. To name the section the user must first toggle multi-section mode ON. This avoids "single-section mode lost forever" — `label = ""` is treated as `null` on save (an empty string never reaches the DB).

**Drag/drop**: Use `vuedraggable` (or hand-rolled HTML5 DnD if dependency unwanted). Same handle component for sections, ingredient rows, step rows.

## Migration of existing recipe `d8ab3718`

After the schema migration runs, `d8ab3718` will have one anonymous section with all 17 ingredients + 6 steps. The `note` field still holds "těsto" / "náplň" / "drobenka" hints. **Manual fix via the new edit form** — toggle "více částí", create three sections, drag rows into them, clean up `note` strings. **Blocked until Phase 3 ships** (drag/drop is part of the form work). On Phase 2 the recipe simply renders as a flat list, identical to today minus the `note` hints. Acceptable wait — the user is the only consumer.

## Tests

**Backend:**
- `recipes::create` with multi-section payload, verify FKs.
- `recipes::update` adding/removing/reordering sections.
- `recipes::get` returns sections in `sort_order`, ingredients per-section in `sort_order`, derived times.
- `ai::ingest::parse_text` on a fixture with sub-headings — section count and per-section ingredient counts match.
- Migration smoke test: data fixture with 2 recipes through migration → both have 1 section, all rows linked.

**Frontend:**
- `IngredientList` snapshot for single-section (no header) and multi-section (with headers).
- `RecipeForm` toggle ON/OFF preserves data round-trip.

## Implementation order

The LLM prompt is the load-bearing piece — manual entry is a minority use case. Iterate on the prompt against real recipes **first**, before investing in the form UX.

1. **Phase 1 — LLM prompt iteration (highest priority).** Land the schema migration and minimal backend CRUD just enough to round-trip a multi-section recipe. Update `ai/ingest.rs` schema + prompt. Ingest a corpus of test URLs covering: simple flat recipes, recipes with 2 sections (typical bake: dough + filling), recipes with 3+ sections (this app's example: dough + filling + streusel), recipes in non-Czech sources. Inspect output, iterate on prompt rules, verify section detection is reliable. Acceptance: re-ingesting `https://www.vareni.cz/recepty/svestkovy-kolac-s-tvarohem/` yields 3 sections with the right ingredient/step distribution.
2. **Phase 2 — read-side frontend.** `IngredientList`, `StepList`, detail/public pages render sections correctly. Section detection from LLM is now visible end-to-end.
3. **Phase 3 — manual entry (lower priority, can ship behind).** `RecipeForm` toggle + section management UI + drag/drop. Functional bar is "lets the user fix what the LLM got wrong"; doesn't need lavish polish.
4. **Phase 4 — polish.** `CookingMode` section header, manual fix of `d8ab3718`, dogfooding tweaks.

## Out of scope (deferred)

- Per-section image / yield / servings.
- URL anchors for sections, table-of-contents on detail page.
- Step-level multimedia.
- Section templates ("Standardní těsto na koláč").
- **LLM chat-driven section split** — the recipe edit chat (`recipe_edit_sessions`) could offer "rozděl tento recept na sekce podle významu" as a guided action, letting the user fix legacy flat recipes (like `d8ab3718`) without manual drag/drop. Defer to a follow-up after sections ship and the chat surface is exercised against real flat recipes.
