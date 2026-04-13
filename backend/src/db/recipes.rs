use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{
    CreateRecipeRequest, IngredientInput, Recipe, RecipeDetail, RecipeIngredient, RecipeStep,
    StepInput, UpdateRecipeRequest,
};

pub async fn create(
    pool: &PgPool,
    owner_id: Uuid,
    req: &CreateRecipeRequest,
) -> Result<RecipeDetail, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let recipe = sqlx::query_as::<_, Recipe>(
        "INSERT INTO recipes (owner_id, title, description, servings, prep_time_min, cook_time_min, emoji, source_type, source_url)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING *",
    )
    .bind(owner_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(req.servings)
    .bind(req.prep_time_min)
    .bind(req.cook_time_min)
    .bind(&req.emoji)
    .bind(&req.source_type)
    .bind(&req.source_url)
    .fetch_one(&mut *tx)
    .await?;

    let ingredients = insert_ingredients(&mut tx, recipe.id, &req.ingredients).await?;
    let steps = insert_steps(&mut tx, recipe.id, &req.steps).await?;
    let tags = if let Some(ref tag_list) = req.tags {
        insert_tags(&mut tx, recipe.id, tag_list).await?;
        tag_list.clone()
    } else {
        vec![]
    };

    tx.commit().await?;

    Ok(RecipeDetail {
        recipe,
        ingredients,
        steps,
        tags,
    })
}

pub async fn get_by_id(pool: &PgPool, id: Uuid) -> Result<Option<RecipeDetail>, sqlx::Error> {
    let recipe = sqlx::query_as::<_, Recipe>("SELECT * FROM recipes WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    let Some(recipe) = recipe else {
        return Ok(None);
    };

    let ingredients = get_ingredients(pool, id).await?;
    let steps = get_steps(pool, id).await?;
    let tags = get_tags(pool, id).await?;

    Ok(Some(RecipeDetail {
        recipe,
        ingredients,
        steps,
        tags,
    }))
}

pub async fn list(
    pool: &PgPool,
    q: Option<&str>,
    tag: Option<&str>,
    sort: &str,
    page: i64,
    per_page: i64,
) -> Result<(Vec<Recipe>, i64), sqlx::Error> {
    let offset = (page - 1) * per_page;

    let (items, total) = if let Some(tag_filter) = tag {
        let sql = format!(
            "SELECT r.* FROM recipes r
             JOIN recipe_tags rt ON r.id = rt.recipe_id
             {join}
             WHERE rt.tag = $1
             {order}
             LIMIT $2 OFFSET $3",
            join = if sort == "least_cooked" {
                "LEFT JOIN (
                    SELECT recipe_id, MAX(date) AS last_date
                    FROM meal_plan WHERE recipe_id IS NOT NULL
                    GROUP BY recipe_id
                 ) mp ON mp.recipe_id = r.id"
            } else {
                ""
            },
            order = match sort {
                "least_cooked" => "ORDER BY mp.last_date ASC NULLS FIRST, r.title ASC",
                "prep_time" => "ORDER BY COALESCE(r.prep_time_min, 0) + COALESCE(r.cook_time_min, 0) ASC, r.title ASC",
                _ => "ORDER BY r.updated_at DESC",
            },
        );
        let items = sqlx::query_as::<_, Recipe>(&sql)
            .bind(tag_filter)
            .bind(per_page)
            .bind(offset)
            .fetch_all(pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM recipes r
             JOIN recipe_tags rt ON r.id = rt.recipe_id
             WHERE rt.tag = $1",
        )
        .bind(tag_filter)
        .fetch_one(pool)
        .await?;

        (items, total)
    } else if let Some(search) = q {
        let pattern = format!("%{search}%");
        let sql = format!(
            "SELECT DISTINCT r.* FROM recipes r
             {join}
             WHERE r.title ILIKE $1 OR r.description ILIKE $1
               OR EXISTS (SELECT 1 FROM recipe_ingredients ri JOIN ingredients i ON ri.ingredient_id = i.id WHERE ri.recipe_id = r.id AND i.name ILIKE $1)
               OR EXISTS (SELECT 1 FROM recipe_tags rt WHERE rt.recipe_id = r.id AND rt.tag ILIKE $1)
             {order}
             LIMIT $2 OFFSET $3",
            join = if sort == "least_cooked" {
                "LEFT JOIN (
                    SELECT recipe_id, MAX(date) AS last_date
                    FROM meal_plan WHERE recipe_id IS NOT NULL
                    GROUP BY recipe_id
                 ) mp ON mp.recipe_id = r.id"
            } else {
                ""
            },
            order = match sort {
                "least_cooked" => "ORDER BY mp.last_date ASC NULLS FIRST, r.title ASC",
                "prep_time" => "ORDER BY COALESCE(r.prep_time_min, 0) + COALESCE(r.cook_time_min, 0) ASC, r.title ASC",
                _ => "ORDER BY r.updated_at DESC",
            },
        );
        let items = sqlx::query_as::<_, Recipe>(&sql)
            .bind(&pattern)
            .bind(per_page)
            .bind(offset)
            .fetch_all(pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM recipes r WHERE r.title ILIKE $1 OR r.description ILIKE $1
               OR EXISTS (SELECT 1 FROM recipe_ingredients ri JOIN ingredients i ON ri.ingredient_id = i.id WHERE ri.recipe_id = r.id AND i.name ILIKE $1)
               OR EXISTS (SELECT 1 FROM recipe_tags rt WHERE rt.recipe_id = r.id AND rt.tag ILIKE $1)",
        )
        .bind(&pattern)
        .fetch_one(pool)
        .await?;

        (items, total)
    } else {
        let sql = format!(
            "SELECT r.* FROM recipes r
             {join}
             {order}
             LIMIT $1 OFFSET $2",
            join = if sort == "least_cooked" {
                "LEFT JOIN (
                    SELECT recipe_id, MAX(date) AS last_date
                    FROM meal_plan WHERE recipe_id IS NOT NULL
                    GROUP BY recipe_id
                 ) mp ON mp.recipe_id = r.id"
            } else {
                ""
            },
            order = match sort {
                "least_cooked" => "ORDER BY mp.last_date ASC NULLS FIRST, r.title ASC",
                "prep_time" => "ORDER BY COALESCE(r.prep_time_min, 0) + COALESCE(r.cook_time_min, 0) ASC, r.title ASC",
                _ => "ORDER BY r.updated_at DESC",
            },
        );
        let items = sqlx::query_as::<_, Recipe>(&sql)
            .bind(per_page)
            .bind(offset)
            .fetch_all(pool)
            .await?;

        let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM recipes")
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
) -> Result<Option<RecipeDetail>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Update recipe fields (only non-None fields)
    let existing = sqlx::query_as::<_, Recipe>("SELECT * FROM recipes WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

    let Some(_) = existing else {
        return Ok(None);
    };

    sqlx::query(
        "UPDATE recipes SET
            title = COALESCE($2, title),
            description = COALESCE($3, description),
            servings = COALESCE($4, servings),
            prep_time_min = COALESCE($5, prep_time_min),
            cook_time_min = COALESCE($6, cook_time_min),
            emoji = COALESCE($7, emoji),
            updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(req.servings)
    .bind(req.prep_time_min)
    .bind(req.cook_time_min)
    .bind(&req.emoji)
    .execute(&mut *tx)
    .await?;

    // Replace ingredients if provided
    if let Some(ref ingredients) = req.ingredients {
        sqlx::query("DELETE FROM recipe_ingredients WHERE recipe_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        insert_ingredients(&mut tx, id, ingredients).await?;
    }

    // Replace steps if provided
    if let Some(ref steps) = req.steps {
        sqlx::query("DELETE FROM recipe_steps WHERE recipe_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        insert_steps(&mut tx, id, steps).await?;
    }

    // Replace tags if provided
    if let Some(ref tags) = req.tags {
        sqlx::query("DELETE FROM recipe_tags WHERE recipe_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        insert_tags(&mut tx, id, tags).await?;
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

pub async fn get_by_slug(pool: &PgPool, slug: &str) -> Result<Option<RecipeDetail>, sqlx::Error> {
    let recipe = sqlx::query_as::<_, Recipe>(
        "SELECT * FROM recipes WHERE public_slug = $1 AND is_public = true",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?;

    let Some(recipe) = recipe else {
        return Ok(None);
    };

    let ingredients = get_ingredients(pool, recipe.id).await?;
    let steps = get_steps(pool, recipe.id).await?;
    let tags = get_tags(pool, recipe.id).await?;

    Ok(Some(RecipeDetail {
        recipe,
        ingredients,
        steps,
        tags,
    }))
}

// ── Helpers ──

async fn insert_ingredients(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    recipe_id: Uuid,
    ingredients: &[IngredientInput],
) -> Result<Vec<RecipeIngredient>, sqlx::Error> {
    let mut result = Vec::new();
    for (i, ing) in ingredients.iter().enumerate() {
        // Upsert ingredient by name
        let ingredient_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO ingredients (name) VALUES ($1)
             ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
             RETURNING id",
        )
        .bind(&ing.name)
        .fetch_one(&mut **tx)
        .await?;

        let ri_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO recipe_ingredients (recipe_id, ingredient_id, amount, unit, note, sort_order)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id",
        )
        .bind(recipe_id)
        .bind(ingredient_id)
        .bind(ing.amount)
        .bind(&ing.unit)
        .bind(&ing.note)
        .bind(i as i32)
        .fetch_one(&mut **tx)
        .await?;

        result.push(RecipeIngredient {
            id: ri_id,
            recipe_id,
            ingredient_id: Some(ingredient_id),
            name: ing.name.clone(),
            amount: ing.amount,
            unit: ing.unit.clone(),
            note: ing.note.clone(),
            sort_order: i as i32,
        });
    }
    Ok(result)
}

async fn insert_steps(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    recipe_id: Uuid,
    steps: &[StepInput],
) -> Result<Vec<RecipeStep>, sqlx::Error> {
    let mut result = Vec::new();
    for step in steps {
        let row = sqlx::query_as::<_, RecipeStep>(
            "INSERT INTO recipe_steps (recipe_id, step_order, instruction)
             VALUES ($1, $2, $3) RETURNING *",
        )
        .bind(recipe_id)
        .bind(step.step_order)
        .bind(&step.instruction)
        .fetch_one(&mut **tx)
        .await?;
        result.push(row);
    }
    Ok(result)
}

async fn insert_tags(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    recipe_id: Uuid,
    tags: &[String],
) -> Result<(), sqlx::Error> {
    for tag in tags {
        sqlx::query(
            "INSERT INTO recipe_tags (recipe_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(recipe_id)
        .bind(tag)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn get_ingredients(
    pool: &PgPool,
    recipe_id: Uuid,
) -> Result<Vec<RecipeIngredient>, sqlx::Error> {
    sqlx::query_as::<_, RecipeIngredient>(
        "SELECT ri.id, ri.recipe_id, ri.ingredient_id, i.name, ri.amount, ri.unit, ri.note, ri.sort_order
         FROM recipe_ingredients ri
         JOIN ingredients i ON ri.ingredient_id = i.id
         WHERE ri.recipe_id = $1
         ORDER BY ri.sort_order",
    )
    .bind(recipe_id)
    .fetch_all(pool)
    .await
}

async fn get_steps(pool: &PgPool, recipe_id: Uuid) -> Result<Vec<RecipeStep>, sqlx::Error> {
    sqlx::query_as::<_, RecipeStep>(
        "SELECT * FROM recipe_steps WHERE recipe_id = $1 ORDER BY step_order",
    )
    .bind(recipe_id)
    .fetch_all(pool)
    .await
}

async fn get_tags(pool: &PgPool, recipe_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar::<_, String>("SELECT tag FROM recipe_tags WHERE recipe_id = $1 ORDER BY tag")
        .bind(recipe_id)
        .fetch_all(pool)
        .await
}
