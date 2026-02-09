-- Migration: File Contributions Table
-- Stores git commit history metrics for each file

-- ==================== File Contributions Table ====================
CREATE TABLE IF NOT EXISTS file_contributions (
    id SERIAL PRIMARY KEY,
    repo_id VARCHAR(255) NOT NULL,
    file_path TEXT NOT NULL,
    author_email VARCHAR(255) NOT NULL,
    author_name VARCHAR(255) NOT NULL,
    commit_count INTEGER NOT NULL DEFAULT 0,
    lines_added INTEGER NOT NULL DEFAULT 0,
    lines_deleted INTEGER NOT NULL DEFAULT 0,
    last_commit_date TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(repo_id, file_path, author_email)
);

CREATE INDEX idx_file_contributions_repo_id ON file_contributions(repo_id);
CREATE INDEX idx_file_contributions_file_path ON file_contributions(file_path);
CREATE INDEX idx_file_contributions_author_email ON file_contributions(author_email);
CREATE INDEX idx_file_contributions_commit_count ON file_contributions(commit_count DESC);

-- ==================== File Metadata Table ====================
-- Aggregated metrics per file
CREATE TABLE IF NOT EXISTS file_metadata (
    id SERIAL PRIMARY KEY,
    repo_id VARCHAR(255) NOT NULL,
    file_path TEXT NOT NULL,
    total_commits INTEGER NOT NULL DEFAULT 0,
    primary_author VARCHAR(255),
    last_modified TIMESTAMP,
    lines_changed_total INTEGER NOT NULL DEFAULT 0,
    contributor_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(repo_id, file_path)
);

CREATE INDEX idx_file_metadata_repo_id ON file_metadata(repo_id);
CREATE INDEX idx_file_metadata_file_path ON file_metadata(file_path);
CREATE INDEX idx_file_metadata_total_commits ON file_metadata(total_commits DESC);
CREATE INDEX idx_file_metadata_last_modified ON file_metadata(last_modified DESC);

-- ==================== Module Boundaries Table ====================
-- Stores detected module boundaries
CREATE TABLE IF NOT EXISTS module_boundaries (
    id SERIAL PRIMARY KEY,
    repo_id VARCHAR(255) NOT NULL,
    boundary_id VARCHAR(255) NOT NULL,
    boundary_name VARCHAR(255) NOT NULL,
    boundary_type VARCHAR(50) NOT NULL,
    boundary_path TEXT NOT NULL,
    layer VARCHAR(50),
    file_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(repo_id, boundary_id),
    CONSTRAINT chk_boundary_type CHECK (boundary_type IN ('physical', 'logical', 'architectural'))
);

CREATE INDEX idx_module_boundaries_repo_id ON module_boundaries(repo_id);
CREATE INDEX idx_module_boundaries_type ON module_boundaries(boundary_type);
CREATE INDEX idx_module_boundaries_layer ON module_boundaries(layer);

COMMENT ON TABLE file_contributions IS 'Individual contributor metrics per file';
COMMENT ON TABLE file_metadata IS 'Aggregated git metrics per file';
COMMENT ON TABLE module_boundaries IS 'Detected module boundaries (physical, logical, architectural)';
