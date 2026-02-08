mod graph_builder;
mod neo4j_storage;
mod parsers;

use anyhow::{Context, Result};
use parsers::{
    javascript::JavaScriptParser, 
    typescript::TypeScriptParser, 
    rust_parser::RustParser,
    go_parser::GoParser,
    python_parser::PythonParser,
    LanguageParser, 
    ParsedFile
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AnalysisJob {
    job_id: String,
    repo_url: String,
    branch: String,
    status: String,
    options: Option<HashMap<String, String>>,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub struct JobUpdatePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct ApiClient {
    client: reqwest::Client,
    base_url: String,
}

impl ApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
        }
    }

    pub async fn update_job(&self, job_id: &str, payload: JobUpdatePayload) -> Result<()> {
        let url = format!("{}/api/v1/jobs/{}", self.base_url, job_id);
        
        let response = self.client.patch(&url)
            .json(&payload)
            .send()
            .await
            .context("Failed to send update request")?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            error!("Failed to update job status: {}", error_text);
            return Err(anyhow::anyhow!("API Error: {}", error_text));
        }

        info!("📊 Updated job {} (status={:?}, progress={:?})", 
              job_id, payload.status, payload.progress);
        
        Ok(())
    }
}

#[derive(Debug)]
struct Config {
    redis_url: String,
    neo4j_uri: String,
    neo4j_user: String,
    neo4j_password: String,
    api_gateway_url: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        Ok(Config {
            redis_url: env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            neo4j_uri: env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string()),
            neo4j_user: env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_string()),
            neo4j_password: env::var("NEO4J_PASSWORD").unwrap_or_else(|_| "password".to_string()),
            api_gateway_url: env::var("API_GATEWAY_URL").unwrap_or_else(|_| "http://localhost:8080".to_string()),
        })
    }
}

/// Connect to Redis with exponential backoff retry logic
async fn connect_redis_with_retry(url: &str, max_retries: u32) -> Result<redis::Client> {
    use tokio::time::{sleep, Duration};

    for attempt in 1..=max_retries {
        info!("🔄 Attempting to connect to Redis at {}... (attempt {}/{})", url, attempt, max_retries);
        
        match redis::Client::open(url) {
            Ok(client) => {
                // Test connection
                match client.get_async_connection().await {
                    Ok(_) => {
                        info!("✅ Successfully connected to Redis");
                        return Ok(client);
                    }
                    Err(e) => {
                        if attempt < max_retries {
                            let wait_time = 2u64.pow(attempt - 1); // 1s, 2s, 4s, 8s
                            warn!("⚠️  Failed to connect to Redis: {}. Retrying in {}s (attempt {}/{})...", 
                                  e, wait_time, attempt, max_retries);
                            sleep(Duration::from_secs(wait_time)).await;
                        } else {
                            error!("❌ Failed to connect to Redis after {} attempts: {}", max_retries, e);
                            return Err(anyhow::anyhow!("Redis connection failed after {} retries: {}", max_retries, e));
                        }
                    }
                }
            }
            Err(e) => {
                if attempt < max_retries {
                    let wait_time = 2u64.pow(attempt - 1);
                    warn!("⚠️  Failed to create Redis client: {}. Retrying in {}s (attempt {}/{})...", 
                          e, wait_time, attempt, max_retries);
                    sleep(Duration::from_secs(wait_time)).await;
                } else {
                    error!("❌ Failed to create Redis client after {} attempts: {}", max_retries, e);
                    return Err(anyhow::anyhow!("Redis client creation failed after {} retries: {}", max_retries, e));
                }
            }
        }
    }

    Err(anyhow::anyhow!("Failed to connect to Redis"))
}

/// Connect to Neo4j with exponential backoff retry logic
async fn connect_neo4j_with_retry(
    uri: &str,
    user: &str,
    password: &str,
    max_retries: u32,
) -> Result<neo4rs::Graph> {
    use tokio::time::{sleep, Duration};

    for attempt in 1..=max_retries {
        info!("🔄 Attempting to connect to Neo4j at {}... (attempt {}/{})", uri, attempt, max_retries);
        
        match neo4rs::Graph::new(uri, user, password).await {
            Ok(graph) => {
                info!("✅ Successfully connected to Neo4j");
                return Ok(graph);
            }
            Err(e) => {
                if attempt < max_retries {
                    let wait_time = 2u64.pow(attempt - 1); // 1s, 2s, 4s, 8s
                    warn!("⚠️  Failed to connect to Neo4j: {}. Retrying in {}s (attempt {}/{})...", 
                          e, wait_time, attempt, max_retries);
                    sleep(Duration::from_secs(wait_time)).await;
                } else {
                    error!("❌ Failed to connect to Neo4j after {} attempts: {}", max_retries, e);
                    return Err(anyhow::anyhow!("Neo4j connection failed after {} retries: {}", max_retries, e));
                }
            }
        }
    }

    Err(anyhow::anyhow!("Failed to connect to Neo4j"))
}


#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("🚀 Ingestion Worker starting...");

    // Load configuration
    let config = Config::from_env()?;
    let api_client = ApiClient::new(config.api_gateway_url.clone());

    // Connect to Redis with retry
    let redis_client = connect_redis_with_retry(&config.redis_url, 4).await?;
    let mut redis_conn = redis_client
        .get_async_connection()
        .await
        .context("Failed to get Redis async connection")?;

    info!("✅ Connected to Redis");

    // Connect to Neo4j with retry
    let neo4j_graph = connect_neo4j_with_retry(
        &config.neo4j_uri,
        &config.neo4j_user,
        &config.neo4j_password,
        4,
    )
    .await?;

    info!("✅ Connected to Neo4j");

    // Setup shutdown signal handler
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::signal;
    
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("🛑 Shutdown signal received, finishing current job...");
                shutdown_clone.store(true, Ordering::SeqCst);
            }
            Err(err) => {
                error!("Failed to listen for shutdown signal: {}", err);
            }
        }
    });

    // Main worker loop
    info!("👂 Listening for jobs on analysis_queue...");
    while !shutdown.load(Ordering::SeqCst) {
        match process_job(&mut redis_conn, &neo4j_graph, &api_client).await {
            Ok(processed) => {
                if !processed {
                    // No job available, sleep briefly
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                }
            }
            Err(e) => {
                error!("Error processing job: {:?}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }

    // Cleanup on shutdown
    info!("🧹 Cleaning up temporary files...");
    cleanup_temp_files().await;
    
    info!("👋 Ingestion Worker shutdown complete");
    Ok(())
}

/// Clean up temporary repository clones
async fn cleanup_temp_files() {
    use std::path::Path;
    use tokio::fs;
    
    let temp_dir = std::env::temp_dir();
    let archmind_pattern = "archmind-";
    
    if let Ok(mut entries) = fs::read_dir(&temp_dir).await {
        let mut cleanup_count = 0;
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(file_name) = entry.file_name().into_string() {
                if file_name.starts_with(archmind_pattern) {
                    if let Err(e) = fs::remove_dir_all(entry.path()).await {
                        warn!("Failed to remove temp dir {}: {}", file_name, e);
                    } else {
                        cleanup_count += 1;
                    }
                }
            }
        }
        if cleanup_count > 0 {
            info!("✅ Cleaned up {} temporary directories", cleanup_count);
        }
    }
}

async fn process_job(
    redis_conn: &mut redis::aio::Connection,
    neo4j_graph: &neo4rs::Graph,
    api_client: &ApiClient,
) -> Result<bool> {
    // Use RPOP instead of BRPOP for compatibility with Redis 3.x (Windows)
    // which doesn't support float timeouts sent by the redis crate
    let result: Option<String> = redis_conn
        .rpop("analysis_queue", None)
        .await
        .context("Failed to pop from Redis queue")?;

    if let Some(job_json) = result {
        // Deserialize job
        let job: AnalysisJob = serde_json::from_str(&job_json)
            .context("Failed to deserialize job")?;

        info!("📝 Processing job: {} for repo: {}", job.job_id, job.repo_url);

        // Update status to PROCESSING (0%)
        let payload = JobUpdatePayload {
            status: Some("PROCESSING".to_string()),
            progress: Some(0),
            result_summary: None,
            error: None,
        };
        
        if let Err(e) = api_client.update_job(&job.job_id, payload).await {
            error!("Failed to update job status to PROCESSING: {:?}", e);
        }

        // Process the job
        match analyze_repository(&job, neo4j_graph, api_client).await {
            Ok(summary) => {
                info!("✅ Successfully processed job: {}", job.job_id);
                // Update status to COMPLETED
                let payload = JobUpdatePayload {
                    status: Some("COMPLETED".to_string()),
                    progress: Some(100),
                    result_summary: Some(summary),
                    error: None,
                };
                if let Err(e) = api_client.update_job(&job.job_id, payload).await {
                    error!("Failed to update job status to COMPLETED: {:?}", e);
                }
            }
            Err(e) => {
                error!("❌ Failed to process job {}: {:?}", job.job_id, e);
                // Update status to FAILED
                let error_msg = format!("{:?}", e);
                let payload = JobUpdatePayload {
                    status: Some("FAILED".to_string()),
                    progress: None,
                    result_summary: None,
                    error: Some(error_msg),
                };
                if let Err(e) = api_client.update_job(&job.job_id, payload).await {
                    error!("Failed to update job status to FAILED: {:?}", e);
                }
            }
        }

        Ok(true)
    } else {
        // No job available
        Ok(false)
    }
}

use git2::{Cred, FetchOptions, RemoteCallbacks};
use std::ops::Deref;
use uuid::Uuid;

struct TempRepo {
    path: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        info!("🧹 Cleaning up temporary repository: {:?}", self.path);
        if let Err(e) = fs::remove_dir_all(&self.path) {
            error!("❌ Failed to cleanup temporary directory {:?}: {:?}", self.path, e);
        }
    }
}

impl AsRef<Path> for TempRepo {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Deref for TempRepo {
    type Target = PathBuf;
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

async fn analyze_repository(
    job: &AnalysisJob, 
    neo4j_graph: &neo4rs::Graph,
    api_client: &ApiClient,
) -> Result<serde_json::Value> {
    info!("🔍 Analyzing repository: {}", job.repo_url);

    // Step 1: Clone repository
    let temp_repo = clone_repository(&job.repo_url, &job.branch, &job.options)?;
    info!("📦 Repository cloned to: {:?}", temp_repo.path);

    // Update progress: 25%
    if let Err(e) = api_client.update_job(&job.job_id, JobUpdatePayload {
        status: None,
        progress: Some(25),
        result_summary: None,
        error: None,
    }).await {
        error!("Failed to update progress to 25%: {:?}", e);
    }

    // Step 2: Parse source files with tree-sitter
    let parsed_files = parse_repository(&temp_repo.path)?;
    info!("📄 Parsed {} files", parsed_files.len());

    // Update progress: 50%
    if let Err(e) = api_client.update_job(&job.job_id, JobUpdatePayload {
        status: None,
        progress: Some(50),
        result_summary: None,
        error: None,
    }).await {
        error!("Failed to update progress to 50%: {:?}", e);
    }

    // Step 3: Build symbol table for cross-file resolution
    let symbol_table = graph_builder::SymbolTable::from_parsed_files(&parsed_files);
    info!("📚 Built symbol table: {} functions, {} classes", 
          symbol_table.functions.len(), 
          symbol_table.classes.len());

    // Step 4: Build dependency graph
    let dep_graph = graph_builder::DependencyGraph::from_parsed_files(&parsed_files, &symbol_table);
    let stats = dep_graph.stats();
    info!("🔗 Built dependency graph: {} nodes, {} edges", 
          dep_graph.nodes.len(), 
          dep_graph.edges.len());

    // Update progress: 75%
    if let Err(e) = api_client.update_job(&job.job_id, JobUpdatePayload {
        status: None,
        progress: Some(75),
        result_summary: None,
        error: None,
    }).await {
        error!("Failed to update progress to 75%: {:?}", e);
    }

    // Step 5: Store in Neo4j (batch operations with transactions)
    neo4j_storage::store_graph(neo4j_graph, &job.job_id, &parsed_files, &dep_graph, None).await?;
    info!("💾 Stored graph data in Neo4j (batch mode)");

    // Update progress: 90%
    if let Err(e) = api_client.update_job(&job.job_id, JobUpdatePayload {
        status: None,
        progress: Some(90),
        result_summary: None,
        error: None,
    }).await {
        error!("Failed to update progress to 90%: {:?}", e);
    }

    // Create result summary
    let summary = serde_json::json!({
        "total_files": parsed_files.len(),
        "total_functions": stats.functions,
        "total_classes": stats.classes,
        "dependencies": stats.imports_edges,
        "complexity_score": 0.0, // Placeholder
        "languages": {} // Placeholder
    });
    
    Ok(summary)
}

fn clone_repository(
    repo_url: &str, 
    branch: &str,
    options: &Option<HashMap<String, String>>
) -> Result<TempRepo> {
    // Generate unique temporary directory
    let tmp_dir = env::temp_dir().join(format!("archmind-repo-{}", Uuid::new_v4()));
    info!("🚀 Cloning {} (branch: {}) to {:?}", repo_url, branch, tmp_dir);

    // Prepare callbacks for authentication
    let mut callbacks = RemoteCallbacks::new();
    
    // Check for git token in options
    if let Some(opts) = options {
        if let Some(token) = opts.get("git_token") {
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                Cred::userpass_plaintext("x-access-token", token)
            });
        }
    }

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_options);

    // Clone the repository
    let repo = builder.clone(repo_url, &tmp_dir)
        .context("Failed to clone repository")?;

    // Checkout specific branch if not default
    let head = repo.head().context("Failed to get HEAD")?;
    let head_name = head.shorthand().unwrap_or("master");

    if head_name != branch {
        info!("🔀 Switching to branch: {}", branch);
        
        // Try the specified branch, then fallback to common branch names
        let branches_to_try = vec![
            branch.to_string(),
            format!("origin/{}", branch),
            "master".to_string(),
            "origin/master".to_string(),
            "main".to_string(),
            "origin/main".to_string(),
        ];
        
        let mut found = None;
        for branch_name in &branches_to_try {
            if let Ok(result) = repo.revparse_ext(branch_name) {
                info!("✅ Found branch: {}", branch_name);
                found = Some(result);
                break;
            }
        }
        
        let (object, reference) = found
            .ok_or_else(|| anyhow::anyhow!("No valid branch found. Tried: {:?}", branches_to_try))?;

        repo.checkout_tree(&object, None)
            .context("Failed to checkout branch tree")?;

        match reference {
            Some(gref) => {
                repo.set_head(gref.name().unwrap())
                    .context("Failed to set HEAD")?;
            }
            None => {
                // If it's a commit hash or tag without ref
                repo.set_head_detached(object.id())
                     .context("Failed to set HEAD detached")?;
            }
        }
    } else {
        info!("✅ Already on branch: {}", head_name);
    }

    Ok(TempRepo { path: tmp_dir })
}

fn parse_repository(repo_path: &std::path::PathBuf) -> Result<Vec<ParsedFile>> {
    let mut parsed_files = Vec::new();
    
    // Initialize parsers
    let js_parser = JavaScriptParser::new()?;
    let ts_parser = TypeScriptParser::new()?;
    let rust_parser = RustParser::new()?;
    let go_parser = GoParser::new()?;
    let py_parser = PythonParser::new()?;
    
    // Walk directory tree
    walk_directory(
        repo_path, 
        &mut parsed_files, 
        &js_parser, 
        &ts_parser,
        &rust_parser,
        &go_parser,
        &py_parser
    )?;
    
    info!("📄 Successfully parsed {} files", parsed_files.len());
    Ok(parsed_files)
}

fn walk_directory(
    dir: &PathBuf,
    parsed_files: &mut Vec<ParsedFile>,
    js_parser: &JavaScriptParser,
    ts_parser: &TypeScriptParser,
    rust_parser: &RustParser,
    go_parser: &GoParser,
    py_parser: &PythonParser,
) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    
    for entry in fs::read_dir(dir).context("Failed to read directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        
        // Skip hidden directories and common ignore patterns
        if let Some(name) = path.file_name() {
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') 
                || name_str == "node_modules"
                || name_str == "target"
                || name_str == "dist"
                || name_str == "build" {
                continue;
            }
        }
        
        if path.is_dir() {
            // Recursively walk subdirectories
            walk_directory(
                &path, 
                parsed_files, 
                js_parser, 
                ts_parser,
                rust_parser,
                go_parser,
                py_parser
            )?;
        } else if path.is_file() {
            // Parse files based on extension
            if let Some(extension) = path.extension() {
                let ext = extension.to_string_lossy().to_lowercase();
                let content = fs::read_to_string(&path)
                    .context(format!("Failed to read file: {:?}", path))?;
                
                let parsed = match ext.as_str() {
                    "js" | "jsx" | "mjs" => {
                        js_parser.parse_file(&path, &content).ok()
                    }
                    "ts" | "tsx" => {
                        ts_parser.parse_file(&path, &content).ok()
                    }
                    "rs" => {
                        rust_parser.parse_file(&path, &content).ok()
                    }
                    "go" => {
                        go_parser.parse_file(&path, &content).ok()
                    }
                    "py" => {
                        py_parser.parse_file(&path, &content).ok()
                    }
                    _ => None,
                };
                
                if let Some(parsed_file) = parsed {
                    info!("✓ Parsed: {:?} ({} functions, {} imports)", 
                          path, 
                          parsed_file.functions.len(),
                          parsed_file.imports.len());
                    parsed_files.push(parsed_file);
                }
            }
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests;
