BEGIN;

CREATE TYPE cook_method AS ENUM ('cooking', 'baking', 'frying', 'steaming', 'other');
ALTER TABLE recipe_sections ADD COLUMN cook_method cook_method;

COMMIT;
