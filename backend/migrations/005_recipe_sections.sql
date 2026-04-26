BEGIN;

CREATE TABLE recipe_sections (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  recipe_id UUID NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
  label TEXT,
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

ALTER TABLE recipe_ingredients ALTER COLUMN section_id SET NOT NULL;
ALTER TABLE recipe_steps        ALTER COLUMN section_id SET NOT NULL;

ALTER TABLE recipe_steps DROP CONSTRAINT recipe_steps_pkey;
ALTER TABLE recipe_steps ADD PRIMARY KEY (section_id, step_order);

ALTER TABLE recipes DROP COLUMN prep_time_min;
ALTER TABLE recipes DROP COLUMN cook_time_min;

COMMIT;
