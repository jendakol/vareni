CREATE TABLE recipe_images (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  recipe_id UUID NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('cover', 'screenshot')),
  width INTEGER NOT NULL,
  height INTEGER NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX recipe_images_one_cover ON recipe_images (recipe_id) WHERE kind = 'cover';
CREATE INDEX recipe_images_recipe ON recipe_images (recipe_id, created_at);
ALTER TABLE recipes DROP COLUMN cover_image_path;
