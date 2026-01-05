use anyhow::{Context, Result};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
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

fn clone_repository(repo_url: &str, branch: &str) -> Result<std::path::PathBuf> {
    // For now, return a mock path
    // In production, use git2 to clone:
    // let repo = git2::Repository::clone(repo_url, &tmp_path)?;
    // repo.set_head(&format!("refs/heads/{}", branch))?;
    
    warn!("⚠️  Repository cloning not yet implemented (mock)");
    Ok(std::path::PathBuf::from("/tmp/mock-repo"))
}

fn parse_repository(repo_path: &std::path::PathBuf) -> Result<Vec<ParsedFile>> {
    // Mock implementation
    // In production:
    // 1. Walk directory tree
    // 2. Identify file types by extension
    // 3. Use appropriate tree-sitter parser
    // 4. Extract AST nodes (functions, classes, imports)
    
    warn!("⚠️  Repository parsing not yet implemented (mock)");
    Ok(vec![ParsedFile {
        path: "src/main.rs".to_string(),
        language: "rust".to_string(),
        functions: vec!["main".to_string(), "process_job".to_string()],
        classes: vec![],
        imports: vec!["tokio".to_string(), "redis".to_string()],
    }])
}

fn extract_dependencies(parsed_files: &[ParsedFile]) -> Result<Vec<Dependency>> {
    // Mock implementation
    // In production:
    // 1. Analyze imports across files
    // 2. Detect function calls
    // 3. Identify class inheritance
    // 4. Map relationships
    
    warn!("⚠️  Dependency extraction not yet implemented (mock)");
    Ok(vec![Dependency {
        from: "main".to_string(),
        to: "process_job".to_string(),
        relationship_type: "CALLS".to_string(),
    }])
}

async fn store_in_neo4j(
    graph: &neo4rs::Graph,
    job_id: &str,
    parsed_files: &[ParsedFile],
    dependencies: &[Dependency],
) -> Result<()> {
    // Mock implementation
    // In production:
    // 1. Create nodes for files, functions, classes
    // 2. Create relationships (CALLS, IMPORTS, INHERITS)
    // 3. Add job metadata
    
    warn!("⚠️  Neo4j storage not yet implemented (mock)");
    
    // Example: Create a simple node
    let query = neo4rs::query("CREATE (j:Job {id: $id, status: 'COMPLETED'})").param("id", job_id);
    graph.run(query).await.context("Failed to create job node")?;
    
    info!("Created job node in Neo4j: {}", job_id);
    Ok(())
}

// Helper structs
#[derive(Debug, Clone)]
struct ParsedFile {
    path: String,
    language: String,
    functions: Vec<String>,
    classes: Vec<String>,
    imports: Vec<String>,
}

#[derive(Debug, Clone)]
struct Dependency {
    from: String,
    to: String,
    relationship_type: String,
}
