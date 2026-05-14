use std::collections::HashMap;

use pgvector::Vector;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{
    CreateRecipeRequest, Recipe, RecipeDetail, RecipeIngredient, RecipeSection,
    RecipeSectionWithContent, RecipeStep, UpdateRecipeRequest,
};

/// Explicit column list for Recipe queries (excludes `embedding` which is handled separately).
/// prep_time_min and cook_time_min are derived from recipe_sections.
const RECIPE_COLUMNS: &str = "r.id, r.owner_id, r.title, r.description, r.servings, \
    (SELECT SUM(prep_time_min)::int FROM recipe_sections WHERE recipe_id = r.id) AS prep_time_min, \
    (SELECT SUM(cook_time_min)::int FROM recipe_sections WHERE recipe_id = r.id) AS cook_time_min, \
    r.source_type, r.source_url, r.emoji, \
    r.cover_image_path, r.is_public, r.public_slug, r.created_at, r.updated_at, \
    r.status, r.discovery_score, r.discovered_at, r.scored_at, r.canonical_name";

pub async fn create(
    pool: &PgPool,
    owner_id: Uuid,
    req: &CreateRecipeRequest,
) -> Result<RecipeDetail, AppError> {
    if req.sections.is_empty() {
        return Err(AppError::BadRequest(
            "Recipe must have at least one section".into(),
        ));
    }

    let mut tx = pool.begin().await?;

    let recipe_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO recipes
          (owner_id, title, description, servings, emoji, source_type, source_url, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, COALESCE($8, 'saved'))
        RETURNING id
        "#,
    )
    .bind(owner_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(req.servings)
    .bind(&req.emoji)
    .bind(req.source_type.as_deref().unwrap_or("manual"))
    .bind(&req.source_url)
    .bind(Option::<String>::None) // status default
    .fetch_one(&mut *tx)
    .await?;

    for section in &req.sections {
        let section_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO recipe_sections
              (recipe_id, label, description, prep_time_min, cook_time_min, cook_method, sort_order)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
        )
        .bind(recipe_id)
        .bind(section.label.as_deref().filter(|s| !s.is_empty()))
        .bind(&section.description)
        .bind(section.prep_time_min)
        .bind(section.cook_time_min)
        .bind(section.cook_method.clone())
        .bind(section.sort_order)
        .fetch_one(&mut *tx)
        .await?;

        for (idx, ing) in section.ingredients.iter().enumerate() {
            let ingredient_id = find_or_create_ingredient(&mut tx, &ing.name).await?;
            sqlx::query(
                r#"
                INSERT INTO recipe_ingredients
                  (recipe_id, section_id, ingredient_id, amount, unit, note, sort_order)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
            )
            .bind(recipe_id)
            .bind(section_id)
            .bind(ingredient_id)
            .bind(ing.amount)
            .bind(&ing.unit)
            .bind(&ing.note)
            .bind(idx as i32)
            .execute(&mut *tx)
            .await?;
        }

        for (step_idx, step) in section.steps.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO recipe_steps
                  (recipe_id, section_id, step_order, instruction)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(recipe_id)
            .bind(section_id)
            .bind((step_idx + 1) as i32) // S7: ignore LLM-supplied step_order, use sequential
            .bind(&step.instruction)
            .execute(&mut *tx)
            .await?;
        }
    }

    if let Some(tags) = &req.tags {
        for tag in tags {
            sqlx::query(
                "INSERT INTO recipe_tags (recipe_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(recipe_id)
            .bind(tag)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    get_by_id(pool, recipe_id).await.map(|opt| opt.unwrap())
}

pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<RecipeDetail>, AppError> {
    let recipe: Option<Recipe> = sqlx::query_as::<_, Recipe>(&format!(
        "SELECT {RECIPE_COLUMNS} FROM recipes r WHERE r.id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some(recipe) = recipe else {
        return Ok(None);
    };

    let detail = assemble_detail(pool, recipe).await?;
    Ok(Some(detail))
}

async fn assemble_detail(pool: &PgPool, recipe: Recipe) -> Result<RecipeDetail, AppError> {
    let id = recipe.id;

    let sections: Vec<RecipeSection> = sqlx::query_as::<_, RecipeSection>(
        r#"
        SELECT id, recipe_id, label, description, prep_time_min, cook_time_min, cook_method, sort_order
        FROM recipe_sections WHERE recipe_id = $1 ORDER BY sort_order, id
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    let ingredients: Vec<RecipeIngredient> = sqlx::query_as::<_, RecipeIngredient>(
        r#"
        SELECT ri.id, ri.recipe_id, ri.section_id, ri.ingredient_id,
               i.name, ri.amount, ri.unit, ri.note, ri.sort_order
        FROM recipe_ingredients ri
        LEFT JOIN ingredients i ON i.id = ri.ingredient_id
        WHERE ri.recipe_id = $1
        ORDER BY ri.section_id, ri.sort_order
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    let steps: Vec<RecipeStep> = sqlx::query_as::<_, RecipeStep>(
        r#"
        SELECT recipe_id, section_id, step_order, instruction
        FROM recipe_steps WHERE recipe_id = $1
        ORDER BY section_id, step_order
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;

    let tags: Vec<String> =
        sqlx::query_scalar("SELECT tag FROM recipe_tags WHERE recipe_id = $1 ORDER BY tag")
            .bind(id)
            .fetch_all(pool)
            .await?;

    // Group ingredients and steps by section_id
    let mut ings_by_section: HashMap<Uuid, Vec<RecipeIngredient>> = HashMap::new();
    for ing in ingredients {
        ings_by_section.entry(ing.section_id).or_default().push(ing);
    }
    let mut steps_by_section: HashMap<Uuid, Vec<RecipeStep>> = HashMap::new();
    for step in steps {
        steps_by_section
            .entry(step.section_id)
            .or_default()
            .push(step);
    }

    let sections_with_content = sections
        .into_iter()
        .map(|s| {
            let sid = s.id;
            RecipeSectionWithContent {
                ingredients: ings_by_section.remove(&sid).unwrap_or_default(),
                steps: steps_by_section.remove(&sid).unwrap_or_default(),
                section: s,
            }
        })
        .collect();

    Ok(RecipeDetail {
        recipe,
        sections: sections_with_content,
        tags,
    })
}

pub async fn list(
    pool: &PgPool,
    q: Option<&str>,
    tag: Option<&str>,
    sort: &str,
    page: i64,
    per_page: i64,
    statuses: &[&str],
) -> Result<(Vec<Recipe>, i64), AppError> {
    let offset = (page - 1) * per_page;
    let statuses_vec: Vec<String> = statuses.iter().map(|s| s.to_string()).collect();

    // S5: sort by total time (prep + cook), matching pre-sections semantics
    let prep_time_order = "(\
        COALESCE((SELECT SUM(prep_time_min) FROM recipe_sections WHERE recipe_id = r.id), 0) \
        + COALESCE((SELECT SUM(cook_time_min) FROM recipe_sections WHERE recipe_id = r.id), 0)\
    ) NULLS LAST";

    let (items, total) = if let Some(tag_filter) = tag {
        let sql = format!(
            "SELECT {RECIPE_COLUMNS} FROM recipes r
             JOIN recipe_tags rt ON r.id = rt.recipe_id
             {join}
             WHERE rt.tag = $1 AND r.status = ANY($4)
             {order}
             LIMIT $2 OFFSET $3",
            join = if sort == "least_cooked" {
                "LEFT JOIN (
                    SELECT recipe_id, MAX(date) AS last_date
                    FROM meal_plan_entries WHERE recipe_id IS NOT NULL
                    GROUP BY recipe_id
                 ) mp ON mp.recipe_id = r.id"
            } else {
                ""
            },
            order = match sort {
                "least_cooked" => "ORDER BY mp.last_date ASC NULLS FIRST, r.title ASC",
                "prep_time" => &format!("ORDER BY {prep_time_order}, r.title ASC"),
                _ => "ORDER BY r.updated_at DESC",
            },
        );
        let items = sqlx::query_as::<_, Recipe>(&sql)
            .bind(tag_filter)
            .bind(per_page)
            .bind(offset)
            .bind(&statuses_vec)
            .fetch_all(pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM recipes r
             JOIN recipe_tags rt ON r.id = rt.recipe_id
             WHERE rt.tag = $1 AND r.status = ANY($2)",
        )
        .bind(tag_filter)
        .bind(&statuses_vec)
        .fetch_one(pool)
        .await?;

        (items, total)
    } else if let Some(search) = q {
        let pattern = format!("%{search}%");
        let sql = format!(
            "SELECT DISTINCT {RECIPE_COLUMNS} FROM recipes r
             {join}
             WHERE (unaccent(r.title) ILIKE unaccent($1) OR unaccent(r.description) ILIKE unaccent($1)
               OR EXISTS (SELECT 1 FROM recipe_ingredients ri JOIN ingredients i ON ri.ingredient_id = i.id WHERE ri.recipe_id = r.id AND unaccent(i.name) ILIKE unaccent($1))
               OR EXISTS (SELECT 1 FROM recipe_tags rt WHERE rt.recipe_id = r.id AND unaccent(rt.tag) ILIKE unaccent($1)))
               AND r.status = ANY($4)
             {order}
             LIMIT $2 OFFSET $3",
            join = if sort == "least_cooked" {
                "LEFT JOIN (
                    SELECT recipe_id, MAX(date) AS last_date
                    FROM meal_plan_entries WHERE recipe_id IS NOT NULL
                    GROUP BY recipe_id
                 ) mp ON mp.recipe_id = r.id"
            } else {
                ""
            },
            order = match sort {
                "least_cooked" => "ORDER BY mp.last_date ASC NULLS FIRST, r.title ASC",
                "prep_time" => &format!("ORDER BY {prep_time_order}, r.title ASC"),
                _ => "ORDER BY r.updated_at DESC",
            },
        );
        let items = sqlx::query_as::<_, Recipe>(&sql)
            .bind(&pattern)
            .bind(per_page)
            .bind(offset)
            .bind(&statuses_vec)
            .fetch_all(pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM recipes r WHERE (unaccent(r.title) ILIKE unaccent($1) OR unaccent(r.description) ILIKE unaccent($1)
               OR EXISTS (SELECT 1 FROM recipe_ingredients ri JOIN ingredients i ON ri.ingredient_id = i.id WHERE ri.recipe_id = r.id AND unaccent(i.name) ILIKE unaccent($1))
               OR EXISTS (SELECT 1 FROM recipe_tags rt WHERE rt.recipe_id = r.id AND unaccent(rt.tag) ILIKE unaccent($1)))
               AND r.status = ANY($2)",
        )
        .bind(&pattern)
        .bind(&statuses_vec)
        .fetch_one(pool)
        .await?;

        (items, total)
    } else {
        let sql = format!(
            "SELECT {RECIPE_COLUMNS} FROM recipes r
             {join}
             WHERE r.status = ANY($3)
             {order}
             LIMIT $1 OFFSET $2",
            join = if sort == "least_cooked" {
                "LEFT JOIN (
                    SELECT recipe_id, MAX(date) AS last_date
                    FROM meal_plan_entries WHERE recipe_id IS NOT NULL
                    GROUP BY recipe_id
                 ) mp ON mp.recipe_id = r.id"
            } else {
                ""
            },
            order = match sort {
                "least_cooked" => "ORDER BY mp.last_date ASC NULLS FIRST, r.title ASC",
                "prep_time" => &format!("ORDER BY {prep_time_order}, r.title ASC"),
                _ => "ORDER BY r.updated_at DESC",
            },
        );
        let items = sqlx::query_as::<_, Recipe>(&sql)
            .bind(per_page)
            .bind(offset)
            .bind(&statuses_vec)
            .fetch_all(pool)
            .await?;

        let total =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM recipes r WHERE r.status = ANY($1)")
                .bind(&statuses_vec)
                .fetch_one(pool)
                .await?;

        (items, total)
    };

    Ok((items, total))
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    req: &UpdateRecipeRequest,
) -> Result<Option<RecipeDetail>, AppError> {
    use std::collections::HashSet;

    let mut tx = pool.begin().await?;

    // B2: existence check inside transaction with FOR UPDATE to close TOCTOU window
    let found: Option<i32> = sqlx::query_scalar("SELECT 1 FROM recipes WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if found.is_none() {
        return Ok(None);
    }

    // B2: always bump updated_at, even for section-only edits
    sqlx::query(
        r#"
        UPDATE recipes SET
          title       = COALESCE($1, title),
          description = COALESCE($2, description),
          servings    = COALESCE($3, servings),
          emoji       = COALESCE($4, emoji),
          updated_at  = now()
        WHERE id = $5
        "#,
    )
    .bind(&req.title)
    .bind(&req.description)
    .bind(req.servings)
    .bind(&req.emoji)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // Tags: full replace if provided
    if let Some(tags) = &req.tags {
        sqlx::query("DELETE FROM recipe_tags WHERE recipe_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for tag in tags {
            sqlx::query("INSERT INTO recipe_tags (recipe_id, tag) VALUES ($1, $2)")
                .bind(id)
                .bind(tag)
                .execute(&mut *tx)
                .await?;
        }
    }

    // Sections: diff if provided
    if let Some(incoming) = &req.sections {
        if incoming.is_empty() {
            return Err(AppError::BadRequest(
                "Recipe must have at least one section".into(),
            ));
        }

        // B3: reject duplicate section ids in payload
        let incoming_ids_vec: Vec<Uuid> = incoming.iter().filter_map(|s| s.id).collect();
        let incoming_ids_set: HashSet<Uuid> = incoming_ids_vec.iter().copied().collect();
        if incoming_ids_set.len() != incoming_ids_vec.len() {
            return Err(AppError::BadRequest(
                "Duplicate section_id in payload".into(),
            ));
        }

        // Validate every incoming section_id belongs to THIS recipe
        let owned_ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM recipe_sections WHERE recipe_id = $1")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
        let owned: std::collections::HashSet<Uuid> = owned_ids.iter().copied().collect();

        for s in incoming {
            if let Some(sid) = s.id
                && !owned.contains(&sid)
            {
                return Err(AppError::BadRequest(format!(
                    "section_id {sid} does not belong to recipe {id}"
                )));
            }
        }

        // Compute deletions: in DB, not in payload (reuse incoming_ids_set from B3 check above)
        let to_delete: Vec<Uuid> = owned.difference(&incoming_ids_set).copied().collect();

        for sid in to_delete {
            sqlx::query("DELETE FROM recipe_sections WHERE id = $1")
                .bind(sid)
                .execute(&mut *tx)
                .await?;
            // CASCADE handles ingredients + steps
        }

        // Upsert each incoming section
        for s in incoming {
            let section_id = match s.id {
                Some(existing) => {
                    sqlx::query(
                        r#"
                        UPDATE recipe_sections SET
                          label = $1, description = $2,
                          prep_time_min = $3, cook_time_min = $4,
                          cook_method = $5, sort_order = $6
                        WHERE id = $7
                        "#,
                    )
                    .bind(s.label.as_deref().filter(|x| !x.is_empty()))
                    .bind(&s.description)
                    .bind(s.prep_time_min)
                    .bind(s.cook_time_min)
                    .bind(s.cook_method.clone())
                    .bind(s.sort_order)
                    .bind(existing)
                    .execute(&mut *tx)
                    .await?;
                    existing
                }
                None => {
                    sqlx::query_scalar::<_, Uuid>(
                        r#"
                    INSERT INTO recipe_sections
                      (recipe_id, label, description, prep_time_min, cook_time_min, cook_method, sort_order)
                    VALUES ($1, $2, $3, $4, $5, $6, $7)
                    RETURNING id
                    "#,
                    )
                    .bind(id)
                    .bind(s.label.as_deref().filter(|x| !x.is_empty()))
                    .bind(&s.description)
                    .bind(s.prep_time_min)
                    .bind(s.cook_time_min)
                    .bind(s.cook_method.clone())
                    .bind(s.sort_order)
                    .fetch_one(&mut *tx)
                    .await?
                }
            };

            // Delete-all-and-insert-all the section's ingredients and steps
            sqlx::query("DELETE FROM recipe_ingredients WHERE section_id = $1")
                .bind(section_id)
                .execute(&mut *tx)
                .await?;
            for (idx, ing) in s.ingredients.iter().enumerate() {
                let ingredient_id = find_or_create_ingredient(&mut tx, &ing.name).await?;
                sqlx::query(
                    r#"
                    INSERT INTO recipe_ingredients
                      (recipe_id, section_id, ingredient_id, amount, unit, note, sort_order)
                    VALUES ($1, $2, $3, $4, $5, $6, $7)
                    "#,
                )
                .bind(id)
                .bind(section_id)
                .bind(ingredient_id)
                .bind(ing.amount)
                .bind(&ing.unit)
                .bind(&ing.note)
                .bind(idx as i32)
                .execute(&mut *tx)
                .await?;
            }

            sqlx::query("DELETE FROM recipe_steps WHERE section_id = $1")
                .bind(section_id)
                .execute(&mut *tx)
                .await?;
            for (step_idx, step) in s.steps.iter().enumerate() {
                sqlx::query(
                    r#"
                    INSERT INTO recipe_steps
                      (recipe_id, section_id, step_order, instruction)
                    VALUES ($1, $2, $3, $4)
                    "#,
                )
                .bind(id)
                .bind(section_id)
                .bind((step_idx + 1) as i32) // S7: ignore LLM-supplied step_order, use sequential
                .bind(&step.instruction)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    tx.commit().await?;
    get_by_id(pool, id).await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM recipes WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_public_slug(pool: &PgPool, id: Uuid, slug: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE recipes SET is_public = true, public_slug = $2 WHERE id = $1")
        .bind(id)
        .bind(slug)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_public_slug(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE recipes SET is_public = false, public_slug = NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_by_slug(pool: &PgPool, slug: &str) -> Result<Option<RecipeDetail>, AppError> {
    let recipe: Option<Recipe> = sqlx::query_as::<_, Recipe>(&format!(
        "SELECT {RECIPE_COLUMNS} FROM recipes r WHERE r.public_slug = $1 AND r.is_public = true"
    ))
    .bind(slug)
    .fetch_optional(pool)
    .await?;

    let Some(recipe) = recipe else {
        return Ok(None);
    };

    let detail = assemble_detail(pool, recipe).await?;
    Ok(Some(detail))
}

// ── Status transitions ──

/// Allowed status transitions (from -> [to]).
fn is_valid_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("discovered", "saved")
            | ("discovered", "rejected")
            | ("discovered", "rejected_similar")
            | ("saved", "tested")
            | ("rejected", "discovered")
            | ("rejected_similar", "discovered")
    )
}

pub async fn update_status(
    pool: &PgPool,
    id: Uuid,
    new_status: &str,
) -> Result<Option<(String, Recipe)>, sqlx::Error> {
    let current = sqlx::query_scalar::<_, String>("SELECT status FROM recipes WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    let Some(current) = current else {
        return Ok(None);
    };

    if !is_valid_transition(&current, new_status) {
        return Err(sqlx::Error::Protocol(format!(
            "Invalid status transition: {} → {}",
            current, new_status
        )));
    }

    sqlx::query("UPDATE recipes SET status = $2, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(new_status)
        .execute(pool)
        .await?;

    // S3: re-query with RECIPE_COLUMNS so derived prep/cook times are computed
    let sql = format!("SELECT {RECIPE_COLUMNS} FROM recipes r WHERE r.id = $1");
    let recipe = sqlx::query_as::<_, Recipe>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?;

    Ok(recipe.map(|r| (current, r)))
}

// ── Embedding queries ──

/// Store an embedding and canonical name for a recipe.
pub async fn set_embedding(
    pool: &PgPool,
    id: Uuid,
    embedding: &[f32],
    canonical_name: &str,
) -> Result<(), sqlx::Error> {
    let vec = Vector::from(embedding.to_vec());
    sqlx::query("UPDATE recipes SET embedding = $2, canonical_name = $3 WHERE id = $1")
        .bind(id)
        .bind(vec)
        .bind(canonical_name)
        .execute(pool)
        .await?;
    Ok(())
}

/// Find the N most similar recipes by embedding cosine distance.
/// Returns (recipe_id, title, canonical_name, similarity_score).
pub async fn find_similar(
    pool: &PgPool,
    embedding: &[f32],
    statuses: &[&str],
    limit: i32,
) -> Result<Vec<(Uuid, String, Option<String>, f64)>, sqlx::Error> {
    let vec = Vector::from(embedding.to_vec());
    let statuses_vec: Vec<String> = statuses.iter().map(|s| s.to_string()).collect();
    let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, f64)>(
        "SELECT id, title, canonical_name, 1 - (embedding <=> $1) AS similarity
         FROM recipes
         WHERE embedding IS NOT NULL AND status = ANY($2)
         ORDER BY embedding <=> $1
         LIMIT $3",
    )
    .bind(vec)
    .bind(&statuses_vec)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Insert a discovered recipe with all discovery fields.
/// B1: accepts sections (preserves multi-section structure from parser).
#[allow(clippy::too_many_arguments)]
pub async fn create_discovered(
    pool: &PgPool,
    owner_id: Uuid,
    title: &str,
    description: Option<&str>,
    source_url: &str,
    canonical_name: &str,
    discovery_score: f32,
    embedding: &[f32],
    servings: Option<i32>,
    tags: &[String],
    sections: &[crate::models::SectionInput],
) -> Result<Recipe, sqlx::Error> {
    let vec = Vector::from(embedding.to_vec());
    let mut tx = pool.begin().await?;

    let recipe_id: Uuid = sqlx::query_scalar(
        "INSERT INTO recipes (owner_id, title, description, source_type, source_url,
                              status, canonical_name, discovery_score, embedding,
                              servings,
                              discovered_at, scored_at)
         VALUES ($1, $2, $3, 'url', $4,
                 'discovered', $5, $6, $7,
                 $8,
                 now(), now())
         RETURNING id",
    )
    .bind(owner_id)
    .bind(title)
    .bind(description)
    .bind(source_url)
    .bind(canonical_name)
    .bind(discovery_score)
    .bind(vec)
    .bind(servings)
    .fetch_one(&mut *tx)
    .await?;

    // B1: insert each section (mirrors create path, preserves multi-section structure)
    for section in sections {
        let section_id: Uuid = sqlx::query_scalar(
            "INSERT INTO recipe_sections (recipe_id, label, description, prep_time_min, cook_time_min, cook_method, sort_order)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id",
        )
        .bind(recipe_id)
        .bind(section.label.as_deref().filter(|s| !s.is_empty()))
        .bind(&section.description)
        .bind(section.prep_time_min)
        .bind(section.cook_time_min)
        .bind(section.cook_method.clone())
        .bind(section.sort_order)
        .fetch_one(&mut *tx)
        .await?;

        for (idx, ing) in section.ingredients.iter().enumerate() {
            let ingredient_id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO ingredients (name) VALUES ($1)
                 ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
                 RETURNING id",
            )
            .bind(&ing.name)
            .fetch_one(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO recipe_ingredients (recipe_id, section_id, ingredient_id, amount, unit, note, sort_order)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(recipe_id)
            .bind(section_id)
            .bind(ingredient_id)
            .bind(ing.amount)
            .bind(&ing.unit)
            .bind(&ing.note)
            .bind(idx as i32)
            .execute(&mut *tx)
            .await?;
        }

        // S7: ignore LLM-supplied step_order, use sequential
        for (step_idx, step) in section.steps.iter().enumerate() {
            sqlx::query(
                "INSERT INTO recipe_steps (recipe_id, section_id, step_order, instruction) VALUES ($1, $2, $3, $4)",
            )
            .bind(recipe_id)
            .bind(section_id)
            .bind((step_idx + 1) as i32)
            .bind(&step.instruction)
            .execute(&mut *tx)
            .await?;
        }
    }

    if !tags.is_empty() {
        for tag in tags {
            sqlx::query(
                "INSERT INTO recipe_tags (recipe_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(recipe_id)
            .bind(tag)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    // Return the recipe with derived times
    let sql = format!("SELECT {RECIPE_COLUMNS} FROM recipes r WHERE r.id = $1");
    let recipe = sqlx::query_as::<_, Recipe>(&sql)
        .bind(recipe_id)
        .fetch_one(pool)
        .await?;

    Ok(recipe)
}

// ── Helpers ──

async fn find_or_create_ingredient(
    conn: &mut sqlx::PgConnection,
    name: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO ingredients (name) VALUES ($1)
         ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
         RETURNING id",
    )
    .bind(name)
    .fetch_one(conn)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_status_transitions() {
        assert!(is_valid_transition("discovered", "saved"));
        assert!(is_valid_transition("discovered", "rejected"));
        assert!(is_valid_transition("discovered", "rejected_similar"));
        assert!(is_valid_transition("saved", "tested"));
        assert!(is_valid_transition("rejected", "discovered"));
        assert!(is_valid_transition("rejected_similar", "discovered"));
    }

    #[test]
    fn invalid_status_transitions() {
        assert!(!is_valid_transition("saved", "discovered"));
        assert!(!is_valid_transition("tested", "saved"));
        assert!(!is_valid_transition("rejected", "saved"));
        assert!(!is_valid_transition("discovered", "tested"));
        assert!(!is_valid_transition("saved", "rejected"));
    }
}
