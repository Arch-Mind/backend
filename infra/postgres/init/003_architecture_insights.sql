CREATE TABLE IF NOT EXISTS architecture_insights (
    id SERIAL PRIMARY KEY,
    repo_id UUID NOT NULL,
    pattern_type TEXT NOT NULL,
    confidence DOUBLE PRECISION NULL,
    summary TEXT NOT NULL,
    generated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS architecture_insights_repo_id_idx
    ON architecture_insights (repo_id, generated_at DESC);
