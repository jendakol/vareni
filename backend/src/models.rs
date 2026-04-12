use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

// -- Users --

#[derive(Clone, FromRow, Serialize)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub created_at: Option<OffsetDateTime>,
}

impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("email", &self.email)
            .field("password_hash", &"[REDACTED]")
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[derive(Debug, Serialize)]
pub struct UserWithRestrictions {
    #[serde(flatten)]
    pub user: User,
    pub dietary_restrictions: Vec<String>,
}

// -- Auth --

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub name: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: User,
}

// -- Recipes --

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Recipe {
    pub id: Uuid,
    pub owner_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub cover_image_path: Option<String>,
    pub is_public: Option<bool>,
    pub public_slug: Option<String>,
    pub created_at: Option<OffsetDateTime>,
    pub updated_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize)]
pub struct RecipeDetail {
    #[serde(flatten)]
    pub recipe: Recipe,
    pub ingredients: Vec<RecipeIngredient>,
    pub steps: Vec<RecipeStep>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RecipeIngredient {
    pub id: Uuid,
    pub recipe_id: Uuid,
    pub ingredient_id: Option<Uuid>,
    pub name: String, // joined from ingredients table
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub sort_order: i32,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RecipeStep {
    pub recipe_id: Uuid,
    pub step_order: i32,
    pub instruction: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRecipeRequest {
    pub title: String,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub tags: Option<Vec<String>>,
    pub ingredients: Vec<IngredientInput>,
    pub steps: Vec<StepInput>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IngredientInput {
    pub name: String,
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StepInput {
    pub step_order: i32,
    pub instruction: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRecipeRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub tags: Option<Vec<String>>,
    pub ingredients: Option<Vec<IngredientInput>>,
    pub steps: Option<Vec<StepInput>>,
}

#[derive(Debug, Deserialize)]
pub struct RecipeListQuery {
    pub q: Option<String>,
    pub tag: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

// -- Meal Plan --

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MealPlanEntry {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub date: time::Date,
    pub meal_type: Option<String>,
    pub recipe_id: Option<Uuid>,
    pub free_text: Option<String>,
    pub servings: Option<i32>,
    pub status: Option<String>,
    pub entry_type: Option<String>,
    pub suggested_by_ai: Option<bool>,
    pub note: Option<String>,
    pub created_at: Option<OffsetDateTime>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMealPlanRequest {
    pub date: String, // YYYY-MM-DD
    pub meal_type: String,
    pub recipe_id: Option<Uuid>,
    pub free_text: Option<String>,
    pub servings: Option<i32>,
    pub status: Option<String>,
    pub entry_type: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMealPlanRequest {
    pub date: Option<String>,
    pub meal_type: Option<String>,
    pub recipe_id: Option<Uuid>,
    pub free_text: Option<String>,
    pub servings: Option<i32>,
    pub status: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MealPlanQuery {
    pub from: String, // YYYY-MM-DD
    pub to: String,   // YYYY-MM-DD
}

#[derive(Debug, Deserialize)]
pub struct MealPlanHistoryQuery {
    pub days: Option<i64>,
}

// -- Push --

#[derive(Debug, Deserialize)]
pub struct PushSubscriptionRequest {
    pub subscription: serde_json::Value,
}

// -- Public --

#[derive(Debug, Serialize)]
pub struct ShareResponse {
    pub share_url: String,
    pub slug: String,
}

// -- Settings --

#[derive(Debug, Deserialize)]
pub struct DietaryRestrictionRequest {
    pub restriction: String,
}

// -- Pagination --

#[derive(Debug, Serialize)]
pub struct Paginated<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}
