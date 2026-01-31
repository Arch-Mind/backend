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

    // Step 3: Extract dependencies and relationships
    let dependencies = extract_dependencies(&parsed_files)?;
    info!("🔗 Extracted {} dependencies", dependencies.len());

    // Step 4: Store in Neo4j
    store_in_neo4j(neo4j_graph, &job.job_id, &parsed_files, &dependencies).await?;
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

fn extract_dependencies(parsed_files: &[ParsedFile]) -> Result<Vec<Dependency>> {
    let mut dependencies = Vec::new();
    
    // Build a map of all functions defined in the codebase (including methods)
    let mut defined_functions: HashMap<String, String> = HashMap::new();
    for file in parsed_files {
        for func in &file.functions {
            defined_functions.insert(func.name.clone(), file.path.clone());
        }
        for class in &file.classes {
            for method in &class.methods {
                 defined_functions.insert(method.name.clone(), file.path.clone());
            }
        }
    }
    
    // Extract dependencies from function calls
    for file in parsed_files {
        // Collect all functions to iterate (standalone + methods)
        let mut all_functions = file.functions.iter().collect::<Vec<_>>();
        for class in &file.classes {
            all_functions.extend(class.methods.iter());
        }

        for func in all_functions {
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
        
        // Create class nodes
        for class in &file.classes {
             let class_query = neo4rs::query(
                 "MERGE (c:Class {name: $name, file: $file})
                  SET c.start_line = $start_line,
                      c.end_line = $end_line,
                      c.job_id = $job_id"
             )
             .param("name", class.name.clone())
             .param("file", file.path.clone())
             .param("start_line", class.start_line as i64)
             .param("end_line", class.end_line as i64)
             .param("job_id", job_id);
             graph.run(class_query).await.context("Failed to create class node")?;

             // Link class to file
             let link_class_query = neo4rs::query(
                 "MATCH (f:File {path: $file}), (c:Class {name: $name, file: $file})
                  MERGE (f)-[:DEFINES]->(c)"
             )
             .param("file", file.path.clone())
             .param("name", class.name.clone());
             graph.run(link_class_query).await.context("Failed to link class to file")?;

             // Create methods and link to class
             for method in &class.methods {
                 let method_query = neo4rs::query(
                     "MERGE (m:Function {name: $name, file: $file})
                      SET m.start_line = $start_line,
                          m.end_line = $end_line,
                          m.job_id = $job_id"
                 )
                 .param("name", method.name.clone())
                 .param("file", file.path.clone())
                 .param("start_line", method.start_line as i64)
                 .param("end_line", method.end_line as i64)
                 .param("job_id", job_id);
                 graph.run(method_query).await.context("Failed to create method node")?;

                 let link_method_query = neo4rs::query(
                     "MATCH (c:Class {name: $cname, file: $file}), (m:Function {name: $mname, file: $file})
                      MERGE (c)-[:DEFINES]->(m)"
                 )
                 .param("cname", class.name.clone())
                 .param("file", file.path.clone())
                 .param("mname", method.name.clone());
                 graph.run(link_method_query).await.context("Failed to link method to class")?;
             }
        }

        // Create function nodes (standalone)
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
