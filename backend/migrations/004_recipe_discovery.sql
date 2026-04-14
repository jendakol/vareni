-- Recipe discovery: status lifecycle, embedding dedup, AI scoring

ALTER TABLE recipes ADD COLUMN status TEXT NOT NULL DEFAULT 'saved';
ALTER TABLE recipes ADD COLUMN embedding vector(384);
ALTER TABLE recipes ADD COLUMN discovery_score REAL;
ALTER TABLE recipes ADD COLUMN discovered_at TIMESTAMPTZ;
ALTER TABLE recipes ADD COLUMN scored_at TIMESTAMPTZ;
ALTER TABLE recipes ADD COLUMN canonical_name TEXT;

-- Backfill: existing recipes are all "tested" (already cooked)
UPDATE recipes SET status = 'tested';

-- Index for status filtering (most queries filter by status)
CREATE INDEX idx_recipes_status ON recipes (status);

-- HNSW index for cosine similarity search on embeddings
CREATE INDEX idx_recipes_embedding ON recipes USING hnsw (embedding vector_cosine_ops);
