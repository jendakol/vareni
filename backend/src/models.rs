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
pub struct UserProfile {
    #[serde(flatten)]
    pub user: User,
    pub dietary_restrictions: Vec<String>,
    pub food_preferences: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "cook_method", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum CookMethod {
    Cooking,
    Baking,
    Frying,
    Steaming,
    Other,
}

impl CookMethod {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cooking" => Some(Self::Cooking),
            "baking" => Some(Self::Baking),
            "frying" => Some(Self::Frying),
            "steaming" => Some(Self::Steaming),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Recipe {
    pub id: Uuid,
    pub owner_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub servings: Option<i32>,
    /// Derived: SUM(prep_time_min) over recipe_sections
    pub prep_time_min: Option<i32>,
    /// Derived: SUM(cook_time_min) over recipe_sections
    pub cook_time_min: Option<i32>,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub emoji: Option<String>,
    pub cover_image_path: Option<String>,
    pub is_public: Option<bool>,
    pub public_slug: Option<String>,
    pub created_at: Option<OffsetDateTime>,
    pub updated_at: Option<OffsetDateTime>,
    pub status: String,
    #[sqlx(skip)]
    #[serde(skip_serializing)]
    pub embedding: Option<()>,
    pub discovery_score: Option<f32>,
    pub discovered_at: Option<OffsetDateTime>,
    pub scored_at: Option<OffsetDateTime>,
    pub canonical_name: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RecipeSection {
    pub id: Uuid,
    pub recipe_id: Uuid,
    pub label: Option<String>,
    pub description: Option<String>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    pub cook_method: Option<CookMethod>,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeSectionWithContent {
    #[serde(flatten)]
    pub section: RecipeSection,
    pub ingredients: Vec<RecipeIngredient>,
    pub steps: Vec<RecipeStep>,
}

#[derive(Debug, Serialize)]
pub struct RecipeDetail {
    #[serde(flatten)]
    pub recipe: Recipe,
    pub sections: Vec<RecipeSectionWithContent>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RecipeIngredient {
    pub id: Uuid,
    pub recipe_id: Uuid,
    pub section_id: Uuid,
    pub ingredient_id: Option<Uuid>,
    pub name: String,
    pub amount: Option<f64>,
    pub unit: Option<String>,
    pub note: Option<String>,
    pub sort_order: i32,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct RecipeStep {
    pub recipe_id: Uuid,
    pub section_id: Uuid,
    pub step_order: i32,
    pub instruction: String,
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SectionInput {
    /// If present, an existing section's id (update path). Absent means insert.
    pub id: Option<Uuid>,
    pub label: Option<String>,
    pub description: Option<String>,
    pub prep_time_min: Option<i32>,
    pub cook_time_min: Option<i32>,
    #[serde(default)]
    pub cook_method: Option<CookMethod>,
    pub sort_order: i32,
    pub ingredients: Vec<IngredientInput>,
    pub steps: Vec<StepInput>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRecipeRequest {
    pub title: String,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub emoji: Option<String>,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub tags: Option<Vec<String>>,
    /// Must contain ≥1 section.
    pub sections: Vec<SectionInput>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRecipeRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub servings: Option<i32>,
    pub emoji: Option<String>,
    pub tags: Option<Vec<String>>,
    /// Full section set after edit. Sections without an `id` are inserts;
    /// sections present in DB but missing from this list are deletes.
    pub sections: Option<Vec<SectionInput>>,
}

#[derive(Debug, Deserialize)]
pub struct RecipeListQuery {
    pub q: Option<String>,
    pub tag: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    /// "recent" (default), "least_cooked", "prep_time"
    pub sort: Option<String>,
    /// Filter by status: "saved", "tested", "discovered", "rejected", "rejected_similar"
    /// Comma-separated for multiple. Default: "saved,tested"
    pub status: Option<String>,
}

// -- Meal Plan --

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MealPlanEntry {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub user_name: Option<String>,
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
    pub recipe_title: Option<String>,
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
    /// Optional: log for a specific user. If omitted, logs for the current user.
    pub for_user_id: Option<Uuid>,
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

#[derive(Debug, Deserialize)]
pub struct FoodPreferenceRequest {
    pub preference: String,
}

// -- Pagination --

#[derive(Debug, Serialize)]
pub struct Paginated<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// -- Discovery --

#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    pub prompt: Option<String>,
    pub count: Option<usize>,
    pub planning_for: Option<String>, // "both" (default) or "me"
}

#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub discovered: Vec<Recipe>,
    pub skipped: SkippedCounts,
    pub errors: Vec<SiteError>,
}

#[derive(Debug, Serialize, Default)]
pub struct SkippedCounts {
    pub duplicate: usize,
    pub restricted: usize,
    pub low_score: usize,
    pub similar_to_rejected: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize)]
pub struct SiteError {
    pub site: String,
    pub error: String,
}

#[derive(Debug, Deserialize)]
pub struct StatusUpdateRequest {
    pub status: String,
}
