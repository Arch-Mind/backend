-- PostgreSQL Database Initialization Script for ArchMind
-- Run this script to create the database schema

-- ==================== Users Table ====================
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    github_id VARCHAR(255) UNIQUE,
    email VARCHAR(255) NOT NULL UNIQUE,
    username VARCHAR(255) NOT NULL,
    avatar_url TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_users_github_id ON users(github_id);
CREATE INDEX idx_users_email ON users(email);

-- ==================== Repositories Table ====================
CREATE TABLE IF NOT EXISTS repositories (
    id SERIAL PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    owner_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_analyzed TIMESTAMP
);

CREATE INDEX idx_repositories_owner_id ON repositories(owner_id);
CREATE INDEX idx_repositories_url ON repositories(url);

-- ==================== Analysis Jobs Table ====================
CREATE TABLE IF NOT EXISTS analysis_jobs (
    id SERIAL PRIMARY KEY,
    job_id VARCHAR(255) NOT NULL UNIQUE,
    repo_url TEXT NOT NULL,
    repo_id INTEGER REFERENCES repositories(id) ON DELETE SET NULL,
    branch VARCHAR(255) NOT NULL DEFAULT 'main',
    status VARCHAR(50) NOT NULL DEFAULT 'QUEUED',
    progress INTEGER DEFAULT 0,
    options JSONB DEFAULT '{}',
    result_summary JSONB,
    error_message TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMP,
    CONSTRAINT chk_status CHECK (status IN ('QUEUED', 'PROCESSING', 'COMPLETED', 'FAILED', 'CANCELLED'))
);

CREATE INDEX idx_analysis_jobs_job_id ON analysis_jobs(job_id);
CREATE INDEX idx_analysis_jobs_status ON analysis_jobs(status);
CREATE INDEX idx_analysis_jobs_repo_id ON analysis_jobs(repo_id);
CREATE INDEX idx_analysis_jobs_created_at ON analysis_jobs(created_at DESC);

-- ==================== Analysis Results Table ====================
CREATE TABLE IF NOT EXISTS analysis_results (
    id SERIAL PRIMARY KEY,
    job_id VARCHAR(255) NOT NULL REFERENCES analysis_jobs(job_id) ON DELETE CASCADE,
    total_files INTEGER DEFAULT 0,
    total_functions INTEGER DEFAULT 0,
    total_classes INTEGER DEFAULT 0,
    total_dependencies INTEGER DEFAULT 0,
    complexity_score DECIMAL(10, 2) DEFAULT 0,
    lines_of_code INTEGER DEFAULT 0,
    languages JSONB DEFAULT '{}',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_analysis_results_job_id ON analysis_results(job_id);

-- ==================== API Keys Table (for future use) ====================
CREATE TABLE IF NOT EXISTS api_keys (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key_hash VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    last_used TIMESTAMP,
    expires_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    revoked BOOLEAN DEFAULT FALSE
);

CREATE INDEX idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX idx_api_keys_key_hash ON api_keys(key_hash);

-- ==================== Webhooks Table (for future use) ====================
CREATE TABLE IF NOT EXISTS webhooks (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    repo_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    secret VARCHAR(255),
    events JSONB NOT NULL DEFAULT '["analysis.completed"]',
    active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_webhooks_user_id ON webhooks(user_id);
CREATE INDEX idx_webhooks_repo_id ON webhooks(repo_id);

-- ==================== Triggers for updated_at ====================
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_repositories_updated_at BEFORE UPDATE ON repositories
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_analysis_jobs_updated_at BEFORE UPDATE ON analysis_jobs
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_webhooks_updated_at BEFORE UPDATE ON webhooks
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ==================== Initial Data (Optional) ====================
-- Insert a default system user
INSERT INTO users (id, github_id, email, username, avatar_url)
VALUES (1, 'system', 'system@archmind.dev', 'system', NULL)
ON CONFLICT (email) DO NOTHING;

-- ==================== Views ====================
-- View for recent jobs with repository info
CREATE OR REPLACE VIEW recent_jobs_view AS
SELECT 
    aj.job_id,
    aj.repo_url,
    aj.branch,
    aj.status,
    aj.created_at,
    aj.completed_at,
    ar.total_files,
    ar.total_functions,
    ar.complexity_score,
    r.name as repo_name,
    u.username as owner_username
FROM analysis_jobs aj
LEFT JOIN analysis_results ar ON aj.job_id = ar.job_id
LEFT JOIN repositories r ON aj.repo_id = r.id
LEFT JOIN users u ON r.owner_id = u.id
ORDER BY aj.created_at DESC;

-- ==================== Permissions (Adjust as needed) ====================
-- GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO archmind_user;
-- GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO archmind_user;

-- ==================== Completed ====================
SELECT 'ArchMind PostgreSQL schema initialized successfully!' AS status;
