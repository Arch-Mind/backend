mod graph_builder;
mod neo4j_storage;
mod parsers;
mod git_analyzer;
mod boundary_detector;
mod dependency_metadata;
mod communication_detector;

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
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};
use dependency_metadata::LibraryDependency;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AnalysisJob {
    job_id: String,
    repo_id: String,
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

#[derive(Debug, Serialize)]
struct GraphPatch {
    changed_files: Vec<String>,
    removed_files: Vec<String>,
    nodes: Vec<PatchNode>,
    edges: Vec<PatchEdge>,
}

#[derive(Debug, Serialize)]
struct PatchNode {
    id: String,
    label: String,
    #[serde(rename = "type")]
    node_type: String,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
    extension: Option<String>,
    language: Option<String>,
    depth: usize,
    #[serde(rename = "filePath")]
    file_path: Option<String>,
    #[serde(rename = "lineNumber")]
    line_number: Option<usize>,
    #[serde(rename = "endLineNumber")]
    end_line_number: Option<usize>,
}

#[derive(Debug, Serialize)]
struct PatchEdge {
    id: String,
    source: String,
    target: String,
    #[serde(rename = "type")]
    edge_type: String,
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

    let (changed_files, removed_files) = extract_webhook_changes(&job.options);
    let incremental_flag = job
        .options
        .as_ref()
        .and_then(|opts| opts.get("incremental"))
        .map(|value| value == "true")
        .unwrap_or(false);
    let incremental = incremental_flag || !changed_files.is_empty() || !removed_files.is_empty();

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
    let parsed_files = if incremental {
        parse_repository_subset(&temp_repo.path, &changed_files)?
    } else {
        parse_repository(&temp_repo.path)?
    };
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

    // Step 4: Analyze git commit history
    let git_contributions = match git_analyzer::GitAnalyzer::new(&temp_repo.path) {
        Ok(analyzer) => {
            match analyzer.analyze_contributions() {
                Ok(contributions) => {
                    info!("📊 Analyzed git history: {} files with {} total commits", 
                          contributions.files.len(), 
                          contributions.total_commits);
                    Some(contributions)
                }
                Err(e) => {
                    warn!("⚠️  Failed to analyze git history: {}. Continuing without git metrics.", e);
                    None
                }
            }
        }
        Err(e) => {
            warn!("⚠️  Failed to open git repository: {}. Continuing without git metrics.", e);
            None
        }
    };

    // Step 5: Detect module boundaries
    let boundary_result = boundary_detector::BoundaryDetector::detect_boundaries(&parsed_files, &temp_repo.path)?;
    info!("🗺️  Detected {} module boundaries", boundary_result.boundaries.len());

    // Step 5b: Collect library dependencies from manifests
    let library_dependencies = collect_library_dependencies(&temp_repo.path)?;
    info!("📦 Detected {} library dependencies", library_dependencies.len());

    // Update progress: 60%
    if let Err(e) = api_client.update_job(&job.job_id, JobUpdatePayload {
        status: None,
        progress: Some(60),
        result_summary: None,
        error: None,
    }).await {
        error!("Failed to update progress to 60%: {:?}", e);
    }

    // Step 5c: Detect communication patterns
    let communication_analysis = communication_detector::CommunicationDetector::detect(&temp_repo.path, &parsed_files)?;
    info!(
        "Detected communication artifacts: {} endpoints, {} rpc services, {} queue usages, {} compose services",
        communication_analysis.endpoints.len(),
        communication_analysis.rpc_services.len(),
        communication_analysis.queues.len(),
        communication_analysis.compose_services.len()
    );

    // Step 6: Build dependency graph
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

    // Step 7: Store in Neo4j (batch operations with transactions)
    if incremental {
        neo4j_storage::store_graph_incremental(
            neo4j_graph,
            &job.job_id,
            &job.repo_id,
            &parsed_files,
            &dep_graph,
            git_contributions.as_ref(),
            &boundary_result,
            &library_dependencies,
            &communication_analysis,
            &changed_files,
            &removed_files,
            None,
        ).await?;
        info!("💾 Stored incremental graph update in Neo4j");
    } else {
        neo4j_storage::store_graph(
            neo4j_graph,
            &job.job_id,
            &job.repo_id,
            &parsed_files,
            &dep_graph,
            git_contributions.as_ref(),
            &boundary_result,
            &library_dependencies,
            &communication_analysis,
            None,
        ).await?;
        info!("💾 Stored graph data in Neo4j (batch mode)");
    }

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
    let mut summary = serde_json::json!({
        "total_files": parsed_files.len(),
        "total_functions": stats.functions,
        "total_classes": stats.classes,
        "dependencies": stats.imports_edges,
        "complexity_score": 0.0, // Placeholder
        "languages": {} // Placeholder
    });

    if incremental {
        let patch = build_graph_patch(&parsed_files, &dep_graph, &changed_files, &removed_files);
        summary["graph_patch"] = serde_json::to_value(&patch)?;
        summary["changed_nodes"] = serde_json::to_value(
            patch.nodes.iter().map(|node| node.id.clone()).collect::<Vec<_>>()
        )?;
        summary["changed_edges"] = serde_json::to_value(
            patch.edges.iter().map(|edge| edge.id.clone()).collect::<Vec<_>>()
        )?;
    }
    
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
        repo_path, // Pass root directory
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

fn parse_repository_subset(repo_path: &PathBuf, files: &[String]) -> Result<Vec<ParsedFile>> {
    let mut parsed_files = Vec::new();

    let js_parser = JavaScriptParser::new()?;
    let ts_parser = TypeScriptParser::new()?;
    let rust_parser = RustParser::new()?;
    let go_parser = GoParser::new()?;
    let py_parser = PythonParser::new()?;

    for file in files {
        let normalized = file.replace("\\", "/");
        let abs_path = repo_path.join(&normalized);
        if !abs_path.is_file() {
            continue;
        }

        let content = fs::read_to_string(&abs_path)
            .context(format!("Failed to read file: {:?}", abs_path))?;
        let relative_path_buf = PathBuf::from(&normalized);
        let ext = abs_path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

        let parsed = match ext.as_str() {
            "js" | "jsx" | "mjs" => Some(js_parser.parse_file(&relative_path_buf, &content)?),
            "ts" | "tsx" => Some(ts_parser.parse_file(&relative_path_buf, &content)?),
            "rs" => Some(rust_parser.parse_file(&relative_path_buf, &content)?),
            "go" => Some(go_parser.parse_file(&relative_path_buf, &content)?),
            "py" => Some(py_parser.parse_file(&relative_path_buf, &content)?),
            _ => None,
        };

        if let Some(parsed) = parsed {
            parsed_files.push(parsed);
        }
    }

    info!("📄 Incremental parse: {} files", parsed_files.len());
    Ok(parsed_files)
}

fn extract_webhook_changes(options: &Option<HashMap<String, String>>) -> (Vec<String>, Vec<String>) {
    let mut changed_files = Vec::new();
    let mut removed_files = Vec::new();

    if let Some(opts) = options {
        if let Some(raw) = opts.get("changed_files") {
            if let Ok(files) = serde_json::from_str::<Vec<String>>(raw) {
                changed_files = files;
            }
        }
        if let Some(raw) = opts.get("removed_files") {
            if let Ok(files) = serde_json::from_str::<Vec<String>>(raw) {
                removed_files = files;
            }
        }
    }

    (changed_files, removed_files)
}

fn build_graph_patch(
    parsed_files: &[ParsedFile],
    dep_graph: &graph_builder::DependencyGraph,
    changed_files: &[String],
    removed_files: &[String],
) -> GraphPatch {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut module_nodes = HashSet::new();

    for file in parsed_files {
        let depth = file.path.matches('/').count();
        let label = file.path.split('/').last().unwrap_or(&file.path).to_string();
        let extension = Path::new(&file.path)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        nodes.push(PatchNode {
            id: file.path.clone(),
            label,
            node_type: "file".to_string(),
            parent_id: None,
            extension,
            language: Some(file.language.clone()),
            depth,
            file_path: Some(file.path.clone()),
            line_number: None,
            end_line_number: None,
        });

        for class in &file.classes {
            nodes.push(PatchNode {
                id: format!("{}::{}", file.path, class.name),
                label: class.name.clone(),
                node_type: "class".to_string(),
                parent_id: Some(file.path.clone()),
                extension: None,
                language: Some(file.language.clone()),
                depth: depth + 1,
                file_path: Some(file.path.clone()),
                line_number: Some(class.start_line),
                end_line_number: Some(class.end_line),
            });
        }

        for func in &file.functions {
            nodes.push(PatchNode {
                id: format!("{}::{}", file.path, func.name),
                label: func.name.clone(),
                node_type: "function".to_string(),
                parent_id: Some(file.path.clone()),
                extension: None,
                language: Some(file.language.clone()),
                depth: depth + 2,
                file_path: Some(file.path.clone()),
                line_number: Some(func.start_line),
                end_line_number: Some(func.end_line),
            });
        }
    }

    for edge in &dep_graph.edges {
        let source = node_id_to_string(&edge.from);
        let target = node_id_to_string(&edge.to);
        let edge_type = edge.edge_type.as_str().to_lowercase();
        let id = format!("{}:{}->{}", edge_type, source, target);

        if let graph_builder::NodeId::Module(name) = &edge.from {
            module_nodes.insert(name.clone());
        }
        if let graph_builder::NodeId::Module(name) = &edge.to {
            module_nodes.insert(name.clone());
        }

        edges.push(PatchEdge {
            id,
            source,
            target,
            edge_type,
        });
    }

    for module in module_nodes {
        nodes.push(PatchNode {
            id: module.clone(),
            label: module.clone(),
            node_type: "module".to_string(),
            parent_id: None,
            extension: None,
            language: None,
            depth: 0,
            file_path: None,
            line_number: None,
            end_line_number: None,
        });
    }

    GraphPatch {
        changed_files: changed_files.to_vec(),
        removed_files: removed_files.to_vec(),
        nodes,
        edges,
    }
}

fn node_id_to_string(node: &graph_builder::NodeId) -> String {
    match node {
        graph_builder::NodeId::File(path) => path.clone(),
        graph_builder::NodeId::Class(path, name) => format!("{}::{}", path, name),
        graph_builder::NodeId::Function(path, name) => format!("{}::{}", path, name),
        graph_builder::NodeId::Module(name) => name.clone(),
    }
}

fn collect_library_dependencies(repo_path: &PathBuf) -> Result<Vec<LibraryDependency>> {
    use std::collections::HashSet;

    let mut manifest_files = Vec::new();
    collect_manifest_files(repo_path, &mut manifest_files)?;

    let mut deps_set: HashSet<(String, Option<String>, String)> = HashSet::new();

    for file in manifest_files {
        let relative_path = file.strip_prefix(repo_path).unwrap_or(&file);
        let source_file = relative_path.to_string_lossy().replace("\\", "/");
        let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let entries = match file_name {
            "package.json" => parse_package_json(&file, &source_file)?,
            "requirements.txt" => parse_requirements_txt(&file, &source_file)?,
            "Cargo.toml" => parse_cargo_toml(&file, &source_file)?,
            "go.mod" => parse_go_mod(&file, &source_file)?,
            _ => Vec::new(),
        };

        for dep in entries {
            deps_set.insert((dep.name, dep.version, dep.source_file));
        }
    }

    let mut dependencies = Vec::new();
    for (name, version, source_file) in deps_set {
        dependencies.push(LibraryDependency {
            name,
            version,
            source_file,
        });
    }

    dependencies.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(dependencies)
}

fn collect_manifest_files(current_dir: &PathBuf, results: &mut Vec<PathBuf>) -> Result<()> {
    if !current_dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(current_dir).context("Failed to read directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if let Some(name) = path.file_name() {
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.')
                || name_str == "node_modules"
                || name_str == "target"
                || name_str == "dist"
                || name_str == "build"
                || name_str == "venv"
                || name_str == "__pycache__" {
                continue;
            }
        }

        if path.is_dir() {
            collect_manifest_files(&path, results)?;
        } else if path.is_file() {
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name == "package.json"
                    || file_name == "requirements.txt"
                    || file_name == "Cargo.toml"
                    || file_name == "go.mod" {
                    results.push(path);
                }
            }
        }
    }

    Ok(())
}

fn parse_package_json(path: &PathBuf, source_file: &str) -> Result<Vec<LibraryDependency>> {
    let content = fs::read_to_string(path).context("Failed to read package.json")?;
    let json: serde_json::Value = serde_json::from_str(&content).context("Failed to parse package.json")?;

    let mut deps = Vec::new();
    for section in ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] {
        if let Some(obj) = json.get(section).and_then(|v| v.as_object()) {
            for (name, value) in obj {
                let version = value.as_str().map(|v| v.to_string());
                deps.push(LibraryDependency {
                    name: name.clone(),
                    version,
                    source_file: source_file.to_string(),
                });
            }
        }
    }

    Ok(deps)
}

fn parse_requirements_txt(path: &PathBuf, source_file: &str) -> Result<Vec<LibraryDependency>> {
    use regex::Regex;

    let content = fs::read_to_string(path).context("Failed to read requirements.txt")?;
    let line_re = Regex::new(r"^\s*([A-Za-z0-9_.\-]+)\s*([=<>!~]+\s*[^\s;]+)?")
        .context("Failed to build requirements regex")?;

    let mut deps = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(cap) = line_re.captures(trimmed) {
            let name = cap.get(1).map(|m| m.as_str().to_string());
            let version = cap.get(2).map(|m| m.as_str().trim().to_string());
            if let Some(name) = name {
                deps.push(LibraryDependency {
                    name,
                    version,
                    source_file: source_file.to_string(),
                });
            }
        }
    }

    Ok(deps)
}

fn parse_cargo_toml(path: &PathBuf, source_file: &str) -> Result<Vec<LibraryDependency>> {
    use regex::Regex;

    let content = fs::read_to_string(path).context("Failed to read Cargo.toml")?;
    let simple_re = Regex::new(r#"^\s*([A-Za-z0-9_\-]+)\s*=\s*\"([^\"]+)\""#)
        .context("Failed to build Cargo.toml regex")?;
    let table_re = Regex::new(r#"^\s*([A-Za-z0-9_\-]+)\s*=\s*\{[^}]*version\s*=\s*\"([^\"]+)\"[^}]*\}"#)
        .context("Failed to build Cargo.toml table regex")?;

    let mut deps = Vec::new();
    let mut in_deps = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_deps = matches!(trimmed, "[dependencies]" | "[dev-dependencies]" | "[build-dependencies]");
            continue;
        }

        if !in_deps || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(cap) = table_re.captures(trimmed) {
            deps.push(LibraryDependency {
                name: cap.get(1).unwrap().as_str().to_string(),
                version: Some(cap.get(2).unwrap().as_str().to_string()),
                source_file: source_file.to_string(),
            });
            continue;
        }

        if let Some(cap) = simple_re.captures(trimmed) {
            deps.push(LibraryDependency {
                name: cap.get(1).unwrap().as_str().to_string(),
                version: Some(cap.get(2).unwrap().as_str().to_string()),
                source_file: source_file.to_string(),
            });
        }
    }

    Ok(deps)
}

fn parse_go_mod(path: &PathBuf, source_file: &str) -> Result<Vec<LibraryDependency>> {
    use regex::Regex;

    let content = fs::read_to_string(path).context("Failed to read go.mod")?;
    let single_re = Regex::new(r"^\s*require\s+([^\s]+)\s+([^\s]+)")
        .context("Failed to build go.mod require regex")?;
    let entry_re = Regex::new(r"^\s*([^\s]+)\s+([^\s]+)")
        .context("Failed to build go.mod entry regex")?;

    let mut deps = Vec::new();
    let mut in_require_block = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("require (") {
            in_require_block = true;
            continue;
        }
        if in_require_block && trimmed.starts_with(')') {
            in_require_block = false;
            continue;
        }

        if let Some(cap) = single_re.captures(trimmed) {
            deps.push(LibraryDependency {
                name: cap.get(1).unwrap().as_str().to_string(),
                version: Some(cap.get(2).unwrap().as_str().to_string()),
                source_file: source_file.to_string(),
            });
            continue;
        }

        if in_require_block {
            if let Some(cap) = entry_re.captures(trimmed) {
                deps.push(LibraryDependency {
                    name: cap.get(1).unwrap().as_str().to_string(),
                    version: Some(cap.get(2).unwrap().as_str().to_string()),
                    source_file: source_file.to_string(),
                });
            }
        }
    }

    Ok(deps)
}

pub(crate) fn walk_directory(
    root_dir: &PathBuf,
    current_dir: &PathBuf,
    parsed_files: &mut Vec<ParsedFile>,
    js_parser: &JavaScriptParser,
    ts_parser: &TypeScriptParser,
    rust_parser: &RustParser,
    go_parser: &GoParser,
    py_parser: &PythonParser,
) -> Result<()> {
    if !current_dir.is_dir() {
        return Ok(());
    }
    
    for entry in fs::read_dir(current_dir).context("Failed to read directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();
        
        // Skip hidden directories and common ignore patterns
        if let Some(name) = path.file_name() {
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') 
                || name_str == "node_modules"
                || name_str == "target"
                || name_str == "dist"
                || name_str == "build"
                || name_str == "venv"
                || name_str == "__pycache__" {
                continue;
            }
        }
        
        if path.is_dir() {
            // Recursively walk subdirectories
            walk_directory(
                root_dir,
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
                
                // Compute relative path for ID consistency
                // e.g., "src/main.rs" instead of "C:\Users\...\src\main.rs"
                let relative_path = path.strip_prefix(root_dir).unwrap_or(&path);
                // Ensure forward slashes for consistency across OS
                let path_str = relative_path.to_string_lossy().replace("\\", "/");
                
                // We fake the path in the parser so that the ParsedFile contains relative path
                // But we read content from the absolute path
                let content = fs::read_to_string(&path)
                    .context(format!("Failed to read file: {:?}", path))?;
                
                // Create a PathBuf from the relative path string for the parser
                let relative_path_buf = PathBuf::from(&path_str);
                
                let parsed = match ext.as_str() {
                    "js" | "jsx" | "mjs" => {
                        js_parser.parse_file(&relative_path_buf, &content).ok()
                    }
                    "ts" | "tsx" => {
                        ts_parser.parse_file(&relative_path_buf, &content).ok()
                    }
                    "rs" => {
                        rust_parser.parse_file(&relative_path_buf, &content).ok()
                    }
                    "go" => {
                        go_parser.parse_file(&relative_path_buf, &content).ok()
                    }
                    "py" => {
                        py_parser.parse_file(&relative_path_buf, &content).ok()
                    }
                    _ => None,
                };
                
                if let Some(mut parsed_file) = parsed {
                    // Double check path is standardized
                    parsed_file.path = path_str;
                    
                    info!("✓ Parsed: {} ({} functions, {} imports)", 
                          parsed_file.path, 
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
