mod parsers;

use anyhow::{Context, Result};
use parsers::{javascript::JavaScriptParser, typescript::TypeScriptParser, LanguageParser, ParsedFile};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use tracing::{error, info, warn};

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

async fn analyze_repository(job: &AnalysisJob, neo4j_graph: &neo4rs::Graph) -> Result<()> {
    info!("🔍 Analyzing repository: {}", job.repo_url);

    // Step 1: Clone repository
    let repo_path = clone_repository(&job.repo_url, &job.branch)?;
    info!("📦 Repository cloned to: {:?}", repo_path);

    // Step 2: Parse source files with tree-sitter
    let parsed_files = parse_repository(&repo_path)?;
    info!("📄 Parsed {} files", parsed_files.len());

    // Step 3: Extract dependencies and relationships
    let dependencies = extract_dependencies(&parsed_files)?;
    info!("🔗 Extracted {} dependencies", dependencies.len());

    // Step 4: Store in Neo4j
    store_in_neo4j(neo4j_graph, &job.job_id, &parsed_files, &dependencies).await?;
    info!("💾 Stored graph data in Neo4j");

    // TODO: Update job status to COMPLETED in PostgreSQL via API call or direct DB connection

    Ok(())
}

fn clone_repository(_repo_url: &str, _branch: &str) -> Result<std::path::PathBuf> {
    // For now, return a mock path
    // In production, use git2 to clone:
    // let repo = git2::Repository::clone(repo_url, &tmp_path)?;
    // repo.set_head(&format!("refs/heads/{}", branch))?;
    
    warn!("⚠️  Repository cloning not yet implemented (mock)");
    Ok(std::path::PathBuf::from("/tmp/mock-repo"))
}

fn parse_repository(repo_path: &std::path::PathBuf) -> Result<Vec<ParsedFile>> {
    let mut parsed_files = Vec::new();
    
    // Initialize parsers
    let js_parser = JavaScriptParser::new()?;
    let ts_parser = TypeScriptParser::new()?;
    
    // Walk directory tree
    walk_directory(repo_path, &mut parsed_files, &js_parser, &ts_parser)?;
    
    info!("📄 Successfully parsed {} files", parsed_files.len());
    Ok(parsed_files)
}

fn walk_directory(
    dir: &PathBuf,
    parsed_files: &mut Vec<ParsedFile>,
    js_parser: &JavaScriptParser,
    ts_parser: &TypeScriptParser,
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
            walk_directory(&path, parsed_files, js_parser, ts_parser)?;
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

fn extract_dependencies(parsed_files: &[ParsedFile]) -> Result<Vec<Dependency>> {
    let mut dependencies = Vec::new();
    
    // Build a map of all functions defined in the codebase
    let mut defined_functions: HashMap<String, String> = HashMap::new();
    for file in parsed_files {
        for func in &file.functions {
            defined_functions.insert(func.name.clone(), file.path.clone());
        }
    }
    
    // Extract dependencies from function calls
    for file in parsed_files {
        for func in &file.functions {
            for call in &func.calls {
                // Check if this call is to a function defined in our codebase
                if let Some(target_file) = defined_functions.get(call) {
                    dependencies.push(Dependency {
                        from: format!("{}::{}", file.path, func.name),
                        to: format!("{}::{}", target_file, call),
                        relationship_type: "CALLS".to_string(),
                        source_file: file.path.clone(),
                        target_file: target_file.clone(),
                    });
                }
            }
        }
        
        // Add import dependencies
        for import in &file.imports {
            dependencies.push(Dependency {
                from: file.path.clone(),
                to: import.clone(),
                relationship_type: "IMPORTS".to_string(),
                source_file: file.path.clone(),
                target_file: import.clone(),
            });
        }
    }
    
    info!("🔗 Extracted {} dependencies", dependencies.len());
    Ok(dependencies)
}

async fn store_in_neo4j(
    graph: &neo4rs::Graph,
    job_id: &str,
    parsed_files: &[ParsedFile],
    dependencies: &[Dependency],
) -> Result<()> {
    info!("💾 Storing graph data in Neo4j...");
    
    // Create job node
    let job_query = neo4rs::query(
        "CREATE (j:Job {id: $id, status: 'COMPLETED', timestamp: datetime()})"
    )
    .param("id", job_id);
    graph.run(job_query).await.context("Failed to create job node")?;
    
    // Create file nodes and function nodes
    for file in parsed_files {
        // Create file node
        let file_query = neo4rs::query(
            "MERGE (f:File {path: $path}) 
             SET f.language = $language, 
                 f.job_id = $job_id"
        )
        .param("path", file.path.clone())
        .param("language", file.language.clone())
        .param("job_id", job_id);
        graph.run(file_query).await.context("Failed to create file node")?;
        
        // Create function nodes
        for func in &file.functions {
            let func_query = neo4rs::query(
                "MERGE (fn:Function {name: $name, file: $file})
                 SET fn.start_line = $start_line,
                     fn.end_line = $end_line,
                     fn.job_id = $job_id"
            )
            .param("name", func.name.clone())
            .param("file", file.path.clone())
            .param("start_line", func.start_line as i64)
            .param("end_line", func.end_line as i64)
            .param("job_id", job_id);
            graph.run(func_query).await.context("Failed to create function node")?;
            
            // Link function to file
            let link_query = neo4rs::query(
                "MATCH (f:File {path: $file}), (fn:Function {name: $func_name, file: $file})
                 MERGE (f)-[:DEFINES]->(fn)"
            )
            .param("file", file.path.clone())
            .param("func_name", func.name.clone());
            graph.run(link_query).await.context("Failed to link function to file")?;
        }
    }
    
    // Create dependency relationships
    for dep in dependencies {
        if dep.relationship_type == "CALLS" {
            let dep_query = neo4rs::query(
                "MATCH (from:Function {name: $from})
                 MATCH (to:Function {name: $to})
                 WHERE from.file = $source_file AND to.file = $target_file
                 MERGE (from)-[:CALLS]->(to)"
            )
            .param("from", dep.from.split("::").last().unwrap_or(&dep.from))
            .param("to", dep.to.split("::").last().unwrap_or(&dep.to))
            .param("source_file", dep.source_file.clone())
            .param("target_file", dep.target_file.clone());
            graph.run(dep_query).await.ok(); // Ignore errors for cross-file calls
        } else if dep.relationship_type == "IMPORTS" {
            let import_query = neo4rs::query(
                "MATCH (f:File {path: $file})
                 MERGE (m:Module {name: $module})
                 MERGE (f)-[:IMPORTS]->(m)"
            )
            .param("file", dep.source_file.clone())
            .param("module", dep.to.clone());
            graph.run(import_query).await.ok(); // Ignore errors
        }
    }
    
    info!("✅ Successfully stored {} files and {} dependencies in Neo4j", 
          parsed_files.len(), 
          dependencies.len());
    Ok(())
}

// Helper structs
#[derive(Debug, Clone)]
struct Dependency {
    from: String,
    to: String,
    relationship_type: String,
    source_file: String,
    target_file: String,
}
