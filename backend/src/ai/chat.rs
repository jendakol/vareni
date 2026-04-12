use crate::ai::client::Tool;

pub fn update_recipe_tool() -> Tool {
    Tool {
        name: "update_recipe".into(),
        description: "Update fields of the current recipe based on the conversation".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "description": { "type": "string" },
                "servings": { "type": "number" },
                "prep_time_min": { "type": "number" },
                "cook_time_min": { "type": "number" },
                "tags": { "type": "array", "items": { "type": "string" } },
                "ingredients": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "amount": { "type": "number" },
                            "unit": { "type": "string" },
                            "note": { "type": "string" }
                        }
                    }
                },
                "steps": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "step_order": { "type": "number" },
                            "instruction": { "type": "string" }
                        }
                    }
                }
            }
        }),
    }
}

pub fn system_prompt(recipe_json: &str) -> String {
    format!(
        "You are a cooking assistant helping edit a recipe. The current recipe is:\n\
         <recipe>{recipe_json}</recipe>\n\
         When the user asks to change something, respond conversationally AND call the \
         update_recipe tool with only the fields that changed."
    )
}
