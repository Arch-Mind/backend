mod graph_builder;
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

#[derive(Debug)]
struct Config {
    redis_url: String,
    neo4j_uri: String,
    neo4j_user: String,
    neo4j_password: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        Ok(Config {
            redis_url: env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            neo4j_uri: env::var("NEO4J_URI").unwrap_or_else(|_| "bolt://localhost:7687".to_string()),
            neo4j_user: env::var("NEO4J_USER").unwrap_or_else(|_| "neo4j".to_string()),
            neo4j_password: env::var("NEO4J_PASSWORD").unwrap_or_else(|_| "password".to_string()),
        })
    }
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

    // Connect to Redis
    let redis_client = redis::Client::open(config.redis_url.as_str())
        .context("Failed to create Redis client")?;
    let mut redis_conn = redis_client
        .get_async_connection()
        .await
        .context("Failed to connect to Redis")?;

    info!("✅ Connected to Redis");

    // Connect to Neo4j
    let neo4j_graph = neo4rs::Graph::new(
        &config.neo4j_uri,
        &config.neo4j_user,
        &config.neo4j_password,
    )
    .await
    .context("Failed to connect to Neo4j")?;

    info!("✅ Connected to Neo4j");

    // Main worker loop
    info!("👂 Listening for jobs on analysis_queue...");
    loop {
        match process_job(&mut redis_conn, &neo4j_graph).await {
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
}

async fn process_job(
    redis_conn: &mut redis::aio::Connection,
    neo4j_graph: &neo4rs::Graph,
) -> Result<bool> {
    // Block and wait for job from Redis queue (BRPOP with 5 second timeout)
    let result: Option<(String, String)> = redis_conn
        .brpop("analysis_queue", 5.0)
        .await
        .context("Failed to pop from Redis queue")?;

    if let Some((_, job_json)) = result {
        // Deserialize job
        let job: AnalysisJob = serde_json::from_str(&job_json)
            .context("Failed to deserialize job")?;

        info!("📝 Processing job: {} for repo: {}", job.job_id, job.repo_url);

        // Process the job
        match analyze_repository(&job, neo4j_graph).await {
            Ok(_) => {
                info!("✅ Successfully processed job: {}", job.job_id);
            }
            Err(e) => {
                error!("❌ Failed to process job {}: {:?}", job.job_id, e);
                // TODO: Update job status to FAILED in PostgreSQL
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

async fn analyze_repository(job: &AnalysisJob, neo4j_graph: &neo4rs::Graph) -> Result<()> {
    info!("🔍 Analyzing repository: {}", job.repo_url);

    // Step 1: Clone repository
    let temp_repo = clone_repository(&job.repo_url, &job.branch, &job.options)?;
    info!("📦 Repository cloned to: {:?}", temp_repo.path);

    // Step 2: Parse source files with tree-sitter
    let parsed_files = parse_repository(&temp_repo.path)?;
    info!("📄 Parsed {} files", parsed_files.len());

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
    info!("   - Files: {}, Classes: {}, Functions: {}, Modules: {}",
          stats.files, stats.classes, stats.functions, stats.modules);
    info!("   - DEFINES: {}, CALLS: {}, IMPORTS: {}, INHERITS: {}, CONTAINS: {}",
          stats.defines_edges, stats.calls_edges, stats.imports_edges, 
          stats.inherits_edges, stats.contains_edges);

    // Step 5: Store in Neo4j
    store_in_neo4j(neo4j_graph, &job.job_id, &dep_graph).await?;
    info!("💾 Stored graph data in Neo4j");

    // TODO: Update job status to COMPLETED in PostgreSQL via API call or direct DB connection

    Ok(())
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
        
        let (object, reference) = repo.revparse_ext(branch).or_else(|_| {
            // Try looking for remote branch
            repo.revparse_ext(&format!("origin/{}", branch))
        }).context(format!("Branch '{}' not found", branch))?;

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



async fn store_in_neo4j(
    graph: &neo4rs::Graph,
    job_id: &str,
    dep_graph: &graph_builder::DependencyGraph,
) -> Result<()> {
    info!("💾 Storing graph data in Neo4j...");

    // Create job node
    let job_query = neo4rs::query(
        "CREATE (j:Job {id: $id, status: 'COMPLETED', timestamp: datetime()})"
    )
    .param("id", job_id);
    graph.run(job_query).await.context("Failed to create job node")?;

    // Create all nodes
    for node in &dep_graph.nodes {
        match node {
            graph_builder::NodeId::File(path) => {
                let query = neo4rs::query(
                    "MERGE (f:File {path: $path})
                     SET f.job_id = $job_id"
                )
                .param("path", path.clone())
                .param("job_id", job_id);
                graph.run(query).await.context("Failed to create file node")?;
            }
            graph_builder::NodeId::Class(file, name) => {
                let query = neo4rs::query(
                    "MERGE (c:Class {name: $name, file: $file})
                     SET c.job_id = $job_id"
                )
                .param("name", name.clone())
                .param("file", file.clone())
                .param("job_id", job_id);
                graph.run(query).await.context("Failed to create class node")?;
            }
            graph_builder::NodeId::Function(file, name) => {
                let query = neo4rs::query(
                    "MERGE (fn:Function {name: $name, file: $file})
                     SET fn.job_id = $job_id"
                )
                .param("name", name.clone())
                .param("file", file.clone())
                .param("job_id", job_id);
                graph.run(query).await.context("Failed to create function node")?;
            }
            graph_builder::NodeId::Module(name) => {
                let query = neo4rs::query(
                    "MERGE (m:Module {name: $name})
                     SET m.job_id = $job_id"
                )
                .param("name", name.clone())
                .param("job_id", job_id);
                graph.run(query).await.context("Failed to create module node")?;
            }
        }
    }

    // Create all edges
    for edge in &dep_graph.edges {
        let edge_type = edge.edge_type.as_str();
        
        let query_str = match (&edge.from, &edge.to) {
            (graph_builder::NodeId::File(from_path), graph_builder::NodeId::Class(to_file, to_name)) => {
                format!(
                    "MATCH (from:File {{path: $from_id}})
                     MATCH (to:Class {{name: $to_name, file: $to_file}})
                     MERGE (from)-[:{}]->(to)", edge_type
                )
            }
            (graph_builder::NodeId::File(from_path), graph_builder::NodeId::Function(to_file, to_name)) => {
                format!(
                    "MATCH (from:File {{path: $from_id}})
                     MATCH (to:Function {{name: $to_name, file: $to_file}})
                     MERGE (from)-[:{}]->(to)", edge_type
                )
            }
            (graph_builder::NodeId::File(_), graph_builder::NodeId::Module(to_name)) => {
                format!(
                    "MATCH (from:File {{path: $from_id}})
                     MATCH (to:Module {{name: $to_name}})
                     MERGE (from)-[:{}]->(to)", edge_type
                )
            }
            (graph_builder::NodeId::Class(from_file, from_name), graph_builder::NodeId::Function(to_file, to_name)) => {
                format!(
                    "MATCH (from:Class {{name: $from_name, file: $from_file}})
                     MATCH (to:Function {{name: $to_name, file: $to_file}})
                     MERGE (from)-[:{}]->(to)", edge_type
                )
            }
            (graph_builder::NodeId::Class(from_file, from_name), graph_builder::NodeId::Class(to_file, to_name)) => {
                format!(
                    "MATCH (from:Class {{name: $from_name, file: $from_file}})
                     MATCH (to:Class {{name: $to_name, file: $to_file}})
                     MERGE (from)-[:{}]->(to)", edge_type
                )
            }
            (graph_builder::NodeId::Class(_, _), graph_builder::NodeId::Module(to_name)) => {
                format!(
                    "MATCH (from:Class {{name: $from_name, file: $from_file}})
                     MATCH (to:Module {{name: $to_name}})
                     MERGE (from)-[:{}]->(to)", edge_type
                )
            }
            (graph_builder::NodeId::Function(from_file, from_name), graph_builder::NodeId::Function(to_file, to_name)) => {
                format!(
                    "MATCH (from:Function {{name: $from_name, file: $from_file}})
                     MATCH (to:Function {{name: $to_name, file: $to_file}})
                     MERGE (from)-[:{}]->(to)", edge_type
                )
            }
            _ => continue, // Skip unsupported edge types
        };

        let mut query = neo4rs::query(&query_str);

        // Add parameters based on node types
        match &edge.from {
            graph_builder::NodeId::File(path) => {
                query = query.param("from_id", path.clone());
            }
            graph_builder::NodeId::Class(file, name) | graph_builder::NodeId::Function(file, name) => {
                query = query.param("from_name", name.clone());
                query = query.param("from_file", file.clone());
            }
            _ => {}
        }

        match &edge.to {
            graph_builder::NodeId::File(path) => {
                query = query.param("to_id", path.clone());
            }
            graph_builder::NodeId::Class(file, name) | graph_builder::NodeId::Function(file, name) => {
                query = query.param("to_name", name.clone());
                query = query.param("to_file", file.clone());
            }
            graph_builder::NodeId::Module(name) => {
                query = query.param("to_name", name.clone());
            }
        }

        // Ignore errors for edge creation (some may not match if nodes don't exist)
        graph.run(query).await.ok();
    }

    info!("✅ Successfully stored {} nodes and {} edges in Neo4j",
          dep_graph.nodes.len(),
          dep_graph.edges.len());
    Ok(())
}
