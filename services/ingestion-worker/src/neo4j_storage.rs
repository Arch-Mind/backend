//! Neo4j Batch Storage
//!
//! Efficient batch storage for dependency graphs using UNWIND queries
//! and transaction support.

use crate::graph_builder::{DependencyGraph, EdgeType, NodeId};
use crate::parsers::{FunctionInfo, ParsedFile};
use crate::git_analyzer::RepoContributions;
use crate::boundary_detector::BoundaryDetectionResult;
use crate::dependency_metadata::LibraryDependency;
use crate::communication_detector::{CommunicationAnalysis, QueueDirection};
use anyhow::{Context, Result};
use neo4rs::query;
use std::collections::{HashMap, HashSet};
use tracing::{info, warn};

// ============================================================================
// Configuration
// ============================================================================

const DEFAULT_BATCH_SIZE: usize = 500;

pub struct BatchConfig {
    pub batch_size: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }
}

// ============================================================================
// Helper: Convert to BoltType-compatible HashMap
// ============================================================================

type BoltMap = HashMap<String, String>;
type BoltMapI64 = HashMap<String, i64>;

fn get_qualified_id(file_path: &str, name: &str) -> String {
    format!("{}::{}", file_path, name)
}

fn file_node_to_map(path: &str, language: &str, job_id: &str, repo_id: &str) -> BoltMap {
    let mut m = HashMap::new();
    m.insert("id".to_string(), path.to_string()); // ID is the relative path
    m.insert("path".to_string(), path.to_string());
    m.insert("language".to_string(), language.to_string());
    m.insert("job_id".to_string(), job_id.to_string());
    m.insert("repo_id".to_string(), repo_id.to_string());
    m
}

fn class_node_to_map(name: &str, file: &str, start_line: usize, end_line: usize, job_id: &str, repo_id: &str) -> HashMap<String, neo4rs::BoltType> {
    let mut m: HashMap<String, neo4rs::BoltType> = HashMap::new();
    let id = get_qualified_id(file, name); // ID is file::name
    m.insert("id".to_string(), id.into());
    m.insert("name".to_string(), name.to_string().into());
    m.insert("file".to_string(), file.to_string().into());
    m.insert("start_line".to_string(), (start_line as i64).into());
    m.insert("end_line".to_string(), (end_line as i64).into());
    m.insert("job_id".to_string(), job_id.to_string().into());
    m.insert("repo_id".to_string(), repo_id.to_string().into());
    m
}

fn function_node_to_map(func: &FunctionInfo, file: &str, job_id: &str, repo_id: &str) -> HashMap<String, neo4rs::BoltType> {
    let mut m: HashMap<String, neo4rs::BoltType> = HashMap::new();
    let id = get_qualified_id(file, &func.name); // ID is file::name
    m.insert("id".to_string(), id.into());
    m.insert("name".to_string(), func.name.clone().into());
    m.insert("file".to_string(), file.to_string().into());
    m.insert("start_line".to_string(), (func.start_line as i64).into());
    m.insert("end_line".to_string(), (func.end_line as i64).into());
    m.insert("params".to_string(), func.params.clone().into());
    m.insert("return_type".to_string(), func.return_type.clone().unwrap_or_default().into());
    m.insert("job_id".to_string(), job_id.to_string().into());
    m.insert("repo_id".to_string(), repo_id.to_string().into());
    m
}

fn module_node_to_map(name: &str, job_id: &str, repo_id: &str) -> BoltMap {
    let mut m = HashMap::new();
    m.insert("name".to_string(), name.to_string());
    m.insert("job_id".to_string(), job_id.to_string());
    m.insert("repo_id".to_string(), repo_id.to_string());
    m
}

async fn delete_file_nodes(txn: &mut neo4rs::Txn, repo_id: &str, files: &[String]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let remove_files = query(
        "UNWIND $paths AS path
         MATCH (f:File {path: path, repo_id: $repo_id})
         DETACH DELETE f"
    )
    .param("paths", files.to_vec())
    .param("repo_id", repo_id);

    txn.run(remove_files)
        .await
        .context("Failed to delete file nodes")?;

    let remove_classes = query(
        "UNWIND $paths AS path
         MATCH (c:Class {file: path, repo_id: $repo_id})
         DETACH DELETE c"
    )
    .param("paths", files.to_vec())
    .param("repo_id", repo_id);

    txn.run(remove_classes)
        .await
        .context("Failed to delete class nodes")?;

    let remove_functions = query(
        "UNWIND $paths AS path
         MATCH (fn:Function {file: path, repo_id: $repo_id})
         DETACH DELETE fn"
    )
    .param("paths", files.to_vec())
    .param("repo_id", repo_id);

    txn.run(remove_functions)
        .await
        .context("Failed to delete function nodes")?;

    Ok(())
}



// ============================================================================
// Main Storage Function
// ============================================================================

/// Store the complete dependency graph in Neo4j using batch operations
pub async fn store_graph(
    graph_db: &neo4rs::Graph,
    job_id: &str,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    dep_graph: &DependencyGraph,
    git_contributions: Option<&RepoContributions>,
    boundary_result: &BoundaryDetectionResult,
    library_dependencies: &[LibraryDependency],
    communication_analysis: &CommunicationAnalysis,
    config: Option<BatchConfig>,
) -> Result<()> {
    let config = config.unwrap_or_default();
    info!("💾 Starting batch Neo4j storage (batch_size={})", config.batch_size);

    // Start a transaction
    let mut txn = graph_db.start_txn().await.context("Failed to start transaction")?;

    // Execute batch operations with error handling
    let result = execute_batch_operations(
        &mut txn, 
        job_id, 
        repo_id, 
        parsed_files, 
        dep_graph, 
        git_contributions,
        boundary_result,
        library_dependencies,
        communication_analysis,
        &config
    ).await;

    match result {
        Ok(_) => {
            txn.commit().await.context("Failed to commit transaction")?;
            info!("✅ Transaction committed successfully");
            Ok(())
        }
        Err(e) => {
            warn!("❌ Error during batch insert, rolling back: {}", e);
            txn.rollback().await.context("Failed to rollback transaction")?;
            Err(e)
        }
    }
}

async fn execute_batch_operations(
    txn: &mut neo4rs::Txn,
    job_id: &str,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    dep_graph: &DependencyGraph,
    git_contributions: Option<&RepoContributions>,
    boundary_result: &BoundaryDetectionResult,
    library_dependencies: &[LibraryDependency],
    communication_analysis: &CommunicationAnalysis,
    config: &BatchConfig,
) -> Result<()> {
    // 1. Create Job node
    create_job_node(txn, job_id, repo_id).await?;

    // 2. Batch insert nodes
    batch_insert_file_nodes(txn, job_id, repo_id, parsed_files, git_contributions, config.batch_size).await?;
    batch_insert_class_nodes(txn, job_id, repo_id, parsed_files, config.batch_size).await?;
    batch_insert_function_nodes(txn, job_id, repo_id, parsed_files, config.batch_size).await?;
    batch_insert_module_nodes(txn, job_id, repo_id, dep_graph, config.batch_size).await?;
    
    // 3. Batch insert boundaries
    batch_insert_boundary_nodes(txn, job_id, repo_id, boundary_result, config.batch_size).await?;

    // 3b. Batch insert library nodes
    batch_insert_library_nodes(txn, job_id, repo_id, library_dependencies, config.batch_size).await?;

    // 4. Batch insert edges
    batch_insert_defines_edges(txn, repo_id, dep_graph, config.batch_size).await?;
    batch_insert_contains_edges(txn, repo_id, dep_graph, config.batch_size).await?;
    batch_insert_calls_edges(txn, repo_id, dep_graph, config.batch_size).await?;
    batch_insert_imports_edges(txn, repo_id, dep_graph, config.batch_size).await?;
    batch_insert_inherits_edges(txn, repo_id, dep_graph, config.batch_size).await?;
    batch_insert_belongs_to_edges(txn, repo_id, boundary_result, config.batch_size).await?;

    // 4b. Batch insert library edges
    batch_insert_library_edges(txn, repo_id, parsed_files, library_dependencies, config.batch_size).await?;

    // 4c. Batch insert data dependency edges (tables)
    batch_insert_table_nodes(txn, repo_id, parsed_files, config.batch_size).await?;
    batch_insert_table_edges(txn, repo_id, parsed_files, config.batch_size).await?;

    // 4d. Batch insert service communication edges
    batch_insert_service_nodes(txn, repo_id, parsed_files, config.batch_size).await?;
    batch_insert_service_edges(txn, repo_id, parsed_files, config.batch_size).await?;

    // 4e. Batch insert communication nodes and edges
    batch_insert_endpoint_nodes(txn, repo_id, communication_analysis, config.batch_size).await?;
    batch_insert_endpoint_edges(txn, repo_id, communication_analysis, config.batch_size).await?;
    batch_insert_rpc_nodes(txn, repo_id, communication_analysis, config.batch_size).await?;
    batch_insert_rpc_edges(txn, repo_id, communication_analysis, config.batch_size).await?;
    batch_insert_queue_nodes(txn, repo_id, communication_analysis, config.batch_size).await?;
    batch_insert_queue_edges(txn, repo_id, communication_analysis, config.batch_size).await?;
    batch_insert_compose_service_nodes(txn, repo_id, communication_analysis, config.batch_size).await?;
    batch_insert_endpoint_service_edges(txn, repo_id, communication_analysis, config.batch_size).await?;
    
    // 5. Create file-to-file dependency edges based on imports
    batch_insert_file_dependencies(txn, repo_id, parsed_files, config.batch_size).await?;

    Ok(())
}

/// Store an incremental graph update for a subset of files
pub async fn store_graph_incremental(
    graph_db: &neo4rs::Graph,
    job_id: &str,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    dep_graph: &DependencyGraph,
    git_contributions: Option<&RepoContributions>,
    boundary_result: &BoundaryDetectionResult,
    library_dependencies: &[LibraryDependency],
    communication_analysis: &CommunicationAnalysis,
    changed_files: &[String],
    removed_files: &[String],
    config: Option<BatchConfig>,
) -> Result<()> {
    let config = config.unwrap_or_default();
    info!("💾 Starting incremental Neo4j storage (batch_size={})", config.batch_size);

    let mut txn = graph_db.start_txn().await.context("Failed to start transaction")?;

    let mut files_to_remove = Vec::new();
    files_to_remove.extend_from_slice(changed_files);
    files_to_remove.extend_from_slice(removed_files);
    files_to_remove.sort();
    files_to_remove.dedup();

    delete_file_nodes(&mut txn, repo_id, &files_to_remove).await?;

    let result = execute_batch_operations(
        &mut txn,
        job_id,
        repo_id,
        parsed_files,
        dep_graph,
        git_contributions,
        boundary_result,
        library_dependencies,
        communication_analysis,
        &config,
    )
    .await;

    match result {
        Ok(_) => {
            txn.commit().await.context("Failed to commit transaction")?;
            info!("✅ Incremental transaction committed successfully");
            Ok(())
        }
        Err(e) => {
            warn!("❌ Error during incremental insert, rolling back: {}", e);
            txn.rollback().await.context("Failed to rollback transaction")?;
            Err(e)
        }
    }
}

// ============================================================================
// Job Node
// ============================================================================

async fn create_job_node(txn: &mut neo4rs::Txn, job_id: &str, repo_id: &str) -> Result<()> {
    let q = query(
        "MERGE (j:Job {id: $id, repo_id: $repo_id})
         SET j.status = 'COMPLETED', j.timestamp = datetime()"
    )
    .param("id", job_id)
    .param("repo_id", repo_id);
    
    txn.run(q).await.context("Failed to create job node")?;
    info!("   Created Job node: {}", job_id);
    Ok(())
}

// ============================================================================
// Batch Node Inserts
// ============================================================================

async fn batch_insert_file_nodes(
    txn: &mut neo4rs::Txn,
    job_id: &str,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    git_contributions: Option<&RepoContributions>,
    batch_size: usize,
) -> Result<()> {
    let nodes: Vec<HashMap<String, neo4rs::BoltType>> = parsed_files
        .iter()
        .map(|f| {
            let mut m: HashMap<String, neo4rs::BoltType> = HashMap::new();
            m.insert("id".to_string(), f.path.clone().into());
            m.insert("path".to_string(), f.path.clone().into());
            m.insert("language".to_string(), f.language.clone().into());
            m.insert("job_id".to_string(), job_id.to_string().into());
            m.insert("repo_id".to_string(), repo_id.to_string().into());
            
            // Add git metrics if available
            if let Some(contributions) = git_contributions {
                if let Some(file_contrib) = contributions.files.get(&f.path) {
                    m.insert("commit_count".to_string(), (file_contrib.commit_count as i64).into());
                    m.insert("last_commit_date".to_string(), 
                             file_contrib.last_modified.to_rfc3339().into());
                    m.insert("primary_author".to_string(), 
                             file_contrib.primary_author.clone().into());
                    m.insert("lines_changed_total".to_string(), 
                             (file_contrib.lines_changed_total as i64).into());
                    
                    let contributors: Vec<String> = file_contrib.contributors
                        .iter()
                        .map(|c| c.email.clone())
                        .collect();
                    m.insert("contributors".to_string(), contributors.into());
                }
            }
            
            m
        })
        .collect();

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (f:File {id: node.id})
             SET f.path = node.path,
                 f.language = node.language,
                 f.job_id = node.job_id,
                 f.repo_id = node.repo_id,
                 f.commit_count = COALESCE(node.commit_count, 0),
                 f.last_commit_date = COALESCE(node.last_commit_date, ''),
                 f.primary_author = COALESCE(node.primary_author, ''),
                 f.lines_changed_total = COALESCE(node.lines_changed_total, 0),
                 f.contributors = COALESCE(node.contributors, [])"
        )
        .param("nodes", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert file nodes")?;
    }
    
    info!("   Inserted {} File nodes", nodes.len());
    Ok(())
}

async fn batch_insert_class_nodes(
    txn: &mut neo4rs::Txn,
    job_id: &str,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<HashMap<String, neo4rs::BoltType>> = Vec::new();
    
    for file in parsed_files {
        for class in &file.classes {
            nodes.push(class_node_to_map(&class.name, &file.path, class.start_line, class.end_line, job_id, repo_id));
        }
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (c:Class {id: node.id})
             SET c.name = node.name,
                 c.file = node.file,
                 c.start_line = node.start_line,
                 c.end_line = node.end_line,
                 c.job_id = node.job_id,
                 c.repo_id = node.repo_id"
        )
        .param("nodes", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert class nodes")?;
    }
    
    info!("   Inserted {} Class nodes", nodes.len());
    Ok(())
}

async fn batch_insert_function_nodes(
    txn: &mut neo4rs::Txn,
    job_id: &str,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<HashMap<String, neo4rs::BoltType>> = Vec::new();
    
    for file in parsed_files {
        // Top-level functions
        for func in &file.functions {
            nodes.push(function_node_to_map(func, &file.path, job_id, repo_id));
        }
        
        // Class methods
        for class in &file.classes {
            for method in &class.methods {
                nodes.push(function_node_to_map(method, &file.path, job_id, repo_id));
            }
        }
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (fn:Function {id: node.id})
             SET fn.name = node.name,
                 fn.file = node.file,
                 fn.start_line = node.start_line,
                 fn.end_line = node.end_line,
                 fn.params = node.params,
                 fn.return_type = node.return_type,
                 fn.job_id = node.job_id,
                 fn.repo_id = node.repo_id"
        )
        .param("nodes", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert function nodes")?;
    }
    
    info!("   Inserted {} Function nodes", nodes.len());
    Ok(())
}

async fn batch_insert_module_nodes(
    txn: &mut neo4rs::Txn,
    job_id: &str,
    repo_id: &str,
    dep_graph: &DependencyGraph,
    batch_size: usize,
) -> Result<()> {
    let nodes: Vec<BoltMap> = dep_graph
        .nodes
        .iter()
        .filter_map(|n| {
            if let NodeId::Module(name) = n {
                Some(module_node_to_map(name, job_id, repo_id))
            } else {
                None
            }
        })
        .collect();

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (m:Module {name: node.name})
             SET m.job_id = node.job_id,
                 m.repo_id = node.repo_id"
        )
        .param("nodes", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert module nodes")?;
    }
    
    info!("   Inserted {} Module nodes", nodes.len());
    Ok(())
}

async fn batch_insert_library_nodes(
    txn: &mut neo4rs::Txn,
    job_id: &str,
    repo_id: &str,
    library_dependencies: &[LibraryDependency],
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<BoltMap> = Vec::new();

    for dep in library_dependencies {
        let mut m = HashMap::new();
        m.insert("name".to_string(), dep.name.clone());
        m.insert("version".to_string(), dep.version.clone().unwrap_or_default());
        m.insert("source_file".to_string(), dep.source_file.clone());
        m.insert("job_id".to_string(), job_id.to_string());
        m.insert("repo_id".to_string(), repo_id.to_string());
        nodes.push(m);
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (l:Library {name: node.name, repo_id: node.repo_id})
             SET l.version = CASE WHEN node.version <> '' THEN node.version ELSE l.version END,
                 l.source_file = node.source_file,
                 l.job_id = node.job_id"
        )
        .param("nodes", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert library nodes")?;
    }

    info!("   Inserted {} Library nodes", nodes.len());
    Ok(())
}

fn normalize_import_to_library(import_path: &str) -> Option<String> {
    let trimmed = import_path.trim().trim_matches('"').trim_matches('`');
    if trimmed.starts_with('.') || trimmed.starts_with('/') {
        return None;
    }

    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.is_empty() {
        return None;
    }

    if trimmed.starts_with('@') && parts.len() >= 2 {
        return Some(format!("{}/{}", parts[0], parts[1]));
    }

    Some(parts[0].to_string())
}

async fn batch_insert_library_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    library_dependencies: &[LibraryDependency],
    batch_size: usize,
) -> Result<()> {
    let mut library_versions = HashMap::new();
    for dep in library_dependencies {
        library_versions.insert(dep.name.clone(), dep.version.clone().unwrap_or_default());
    }

    let mut edges: Vec<BoltMap> = Vec::new();
    for file in parsed_files {
        for import in &file.imports {
            if let Some(lib_name) = normalize_import_to_library(import) {
                if library_versions.contains_key(&lib_name) {
                    let mut m = HashMap::new();
                    m.insert("file_path".to_string(), file.path.clone());
                    m.insert("library_name".to_string(), lib_name.clone());
                    m.insert(
                        "library_version".to_string(),
                        library_versions.get(&lib_name).cloned().unwrap_or_default(),
                    );
                    m.insert("repo_id".to_string(), repo_id.to_string());
                    edges.push(m);
                }
            }
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path, repo_id: edge.repo_id})
             MATCH (l:Library {name: edge.library_name, repo_id: edge.repo_id})
             MERGE (f)-[r:DEPENDS_ON]->(l)
             SET r.type = 'library',
                 r.version = edge.library_version"
        )
        .param("edges", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert library edges")?;
    }

    info!("   Created {} Library DEPENDS_ON edges", edges.len());
    Ok(())
}

async fn batch_insert_table_nodes(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<BoltMap> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for file in parsed_files {
        for table in &file.data_tables {
            if seen.insert(table.clone()) {
                let mut m = HashMap::new();
                m.insert("name".to_string(), table.clone());
                m.insert("repo_id".to_string(), repo_id.to_string());
                nodes.push(m);
            }
        }
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (t:Table {name: node.name, repo_id: node.repo_id})"
        )
        .param("nodes", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert table nodes")?;
    }

    info!("   Inserted {} Table nodes", nodes.len());
    Ok(())
}

async fn batch_insert_table_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    batch_size: usize,
) -> Result<()> {
    let mut edges: Vec<BoltMap> = Vec::new();
    for file in parsed_files {
        for table in &file.data_tables {
            let mut m = HashMap::new();
            m.insert("file_path".to_string(), file.path.clone());
            m.insert("table_name".to_string(), table.clone());
            m.insert("repo_id".to_string(), repo_id.to_string());
            edges.push(m);
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path, repo_id: edge.repo_id})
             MATCH (t:Table {name: edge.table_name, repo_id: edge.repo_id})
             MERGE (f)-[:USES_TABLE]->(t)"
        )
        .param("edges", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert table edges")?;
    }

    info!("   Created {} USES_TABLE edges", edges.len());
    Ok(())
}

async fn batch_insert_service_nodes(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<BoltMap> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for file in parsed_files {
        for service in &file.service_calls {
            let key = format!("{}::{}", service.protocol, service.target);
            if seen.insert(key) {
                let mut m = HashMap::new();
                m.insert("name".to_string(), service.target.clone());
                m.insert("protocol".to_string(), service.protocol.clone());
                m.insert("repo_id".to_string(), repo_id.to_string());
                nodes.push(m);
            }
        }
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (s:Service {name: node.name, protocol: node.protocol, repo_id: node.repo_id})"
        )
        .param("nodes", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert service nodes")?;
    }

    info!("   Inserted {} Service nodes", nodes.len());
    Ok(())
}

async fn batch_insert_service_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    batch_size: usize,
) -> Result<()> {
    let mut edges: Vec<BoltMap> = Vec::new();
    for file in parsed_files {
        for service in &file.service_calls {
            let mut m = HashMap::new();
            m.insert("file_path".to_string(), file.path.clone());
            m.insert("service_name".to_string(), service.target.clone());
            m.insert("service_protocol".to_string(), service.protocol.clone());
            m.insert("repo_id".to_string(), repo_id.to_string());
            edges.push(m);
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path, repo_id: edge.repo_id})
             MATCH (s:Service {name: edge.service_name, protocol: edge.service_protocol, repo_id: edge.repo_id})
             MERGE (f)-[:CALLS_SERVICE]->(s)"
        )
        .param("edges", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert service edges")?;
    }

    info!("   Created {} CALLS_SERVICE edges", edges.len());
    Ok(())
}

async fn batch_insert_endpoint_nodes(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    communication_analysis: &CommunicationAnalysis,
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<BoltMap> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for endpoint in &communication_analysis.endpoints {
        let key = format!("{}::{}", endpoint.method, endpoint.url);
        if seen.insert(key) {
            let mut m = HashMap::new();
            m.insert("url".to_string(), endpoint.url.clone());
            m.insert("method".to_string(), endpoint.method.clone());
            m.insert("host".to_string(), endpoint.host.clone().unwrap_or_default());
            m.insert("repo_id".to_string(), repo_id.to_string());
            nodes.push(m);
        }
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (e:Endpoint {url: node.url, method: node.method, repo_id: node.repo_id})
             SET e.host = node.host"
        )
        .param("nodes", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert Endpoint nodes")?;
    }

    info!("   Inserted {} Endpoint nodes", nodes.len());
    Ok(())
}

async fn batch_insert_endpoint_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    communication_analysis: &CommunicationAnalysis,
    batch_size: usize,
) -> Result<()> {
    let mut edges: Vec<BoltMap> = Vec::new();

    for endpoint in &communication_analysis.endpoints {
        let mut m = HashMap::new();
        m.insert("file_path".to_string(), endpoint.file_path.clone());
        m.insert("url".to_string(), endpoint.url.clone());
        m.insert("method".to_string(), endpoint.method.clone());
        m.insert("repo_id".to_string(), repo_id.to_string());
        edges.push(m);
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path, repo_id: edge.repo_id})
             MATCH (e:Endpoint {url: edge.url, method: edge.method, repo_id: edge.repo_id})
             MERGE (f)-[:CALLS_ENDPOINT]->(e)"
        )
        .param("edges", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert CALLS_ENDPOINT edges")?;
    }

    info!("   Created {} CALLS_ENDPOINT edges", edges.len());
    Ok(())
}

async fn batch_insert_rpc_nodes(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    communication_analysis: &CommunicationAnalysis,
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<BoltMap> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for rpc in &communication_analysis.rpc_services {
        if seen.insert(rpc.service_name.clone()) {
            let mut m = HashMap::new();
            m.insert("name".to_string(), rpc.service_name.clone());
            m.insert("repo_id".to_string(), repo_id.to_string());
            nodes.push(m);
        }
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (r:RpcService {name: node.name, repo_id: node.repo_id})"
        )
        .param("nodes", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert RpcService nodes")?;
    }

    info!("   Inserted {} RpcService nodes", nodes.len());
    Ok(())
}

async fn batch_insert_rpc_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    communication_analysis: &CommunicationAnalysis,
    batch_size: usize,
) -> Result<()> {
    let mut edges: Vec<BoltMap> = Vec::new();

    for rpc in &communication_analysis.rpc_services {
        let mut m = HashMap::new();
        m.insert("file_path".to_string(), rpc.file_path.clone());
        m.insert("service_name".to_string(), rpc.service_name.clone());
        m.insert("repo_id".to_string(), repo_id.to_string());
        edges.push(m);
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path, repo_id: edge.repo_id})
             MATCH (r:RpcService {name: edge.service_name, repo_id: edge.repo_id})
             MERGE (f)-[:CALLS_RPC]->(r)"
        )
        .param("edges", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert CALLS_RPC edges")?;
    }

    info!("   Created {} CALLS_RPC edges", edges.len());
    Ok(())
}

async fn batch_insert_queue_nodes(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    communication_analysis: &CommunicationAnalysis,
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<BoltMap> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for queue in &communication_analysis.queues {
        if seen.insert(queue.topic.clone()) {
            let mut m = HashMap::new();
            m.insert("topic".to_string(), queue.topic.clone());
            m.insert("repo_id".to_string(), repo_id.to_string());
            nodes.push(m);
        }
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (q:MessageQueue {topic: node.topic, repo_id: node.repo_id})"
        )
        .param("nodes", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert MessageQueue nodes")?;
    }

    info!("   Inserted {} MessageQueue nodes", nodes.len());
    Ok(())
}

async fn batch_insert_queue_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    communication_analysis: &CommunicationAnalysis,
    batch_size: usize,
) -> Result<()> {
    let mut publish_edges: Vec<BoltMap> = Vec::new();
    let mut consume_edges: Vec<BoltMap> = Vec::new();

    for queue in &communication_analysis.queues {
        let mut m = HashMap::new();
        m.insert("file_path".to_string(), queue.file_path.clone());
        m.insert("topic".to_string(), queue.topic.clone());
        m.insert("repo_id".to_string(), repo_id.to_string());
        match queue.direction {
            QueueDirection::Publish => publish_edges.push(m),
            QueueDirection::Consume => consume_edges.push(m),
        }
    }

    for chunk in publish_edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path, repo_id: edge.repo_id})
             MATCH (q:MessageQueue {topic: edge.topic, repo_id: edge.repo_id})
             MERGE (f)-[:PUBLISHES_TO]->(q)"
        )
        .param("edges", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert PUBLISHES_TO edges")?;
    }

    for chunk in consume_edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path, repo_id: edge.repo_id})
             MATCH (q:MessageQueue {topic: edge.topic, repo_id: edge.repo_id})
             MERGE (f)-[:CONSUMES_FROM]->(q)"
        )
        .param("edges", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert CONSUMES_FROM edges")?;
    }

    info!(
        "   Created {} PUBLISHES_TO and {} CONSUMES_FROM edges",
        publish_edges.len(),
        consume_edges.len()
    );
    Ok(())
}

async fn batch_insert_compose_service_nodes(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    communication_analysis: &CommunicationAnalysis,
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<HashMap<String, neo4rs::BoltType>> = Vec::new();

    for service in &communication_analysis.compose_services {
        let mut m: HashMap<String, neo4rs::BoltType> = HashMap::new();
        m.insert("name".to_string(), service.name.clone().into());
        m.insert("ports".to_string(), service.ports.clone().into());
        m.insert("repo_id".to_string(), repo_id.to_string().into());
        nodes.push(m);
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (s:ComposeService {name: node.name, repo_id: node.repo_id})
             SET s.ports = node.ports"
        )
        .param("nodes", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert ComposeService nodes")?;
    }

    info!("   Inserted {} ComposeService nodes", nodes.len());
    Ok(())
}

async fn batch_insert_endpoint_service_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    communication_analysis: &CommunicationAnalysis,
    batch_size: usize,
) -> Result<()> {
    let mut edges: Vec<BoltMap> = Vec::new();
    let mut service_names: HashSet<String> = HashSet::new();

    for service in &communication_analysis.compose_services {
        service_names.insert(service.name.clone());
    }

    for endpoint in &communication_analysis.endpoints {
        if let Some(host) = endpoint.host.as_ref() {
            for service_name in &service_names {
                if host.contains(service_name) {
                    let mut m = HashMap::new();
                    m.insert("url".to_string(), endpoint.url.clone());
                    m.insert("method".to_string(), endpoint.method.clone());
                    m.insert("service_name".to_string(), service_name.clone());
                    m.insert("repo_id".to_string(), repo_id.to_string());
                    edges.push(m);
                }
            }
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (e:Endpoint {url: edge.url, method: edge.method, repo_id: edge.repo_id})
             MATCH (s:ComposeService {name: edge.service_name, repo_id: edge.repo_id})
             MERGE (e)-[:EXPOSED_BY]->(s)"
        )
        .param("edges", chunk.to_vec());

        txn.run(q).await.context("Failed to batch insert EXPOSED_BY edges")?;
    }

    info!("   Created {} EXPOSED_BY edges", edges.len());
    Ok(())
}

// ============================================================================
// Batch Edge Inserts
// ============================================================================

async fn batch_insert_defines_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    dep_graph: &DependencyGraph,
    batch_size: usize,
) -> Result<()> {
    let mut file_to_class: Vec<BoltMap> = Vec::new();
    let mut file_to_func: Vec<BoltMap> = Vec::new();
    
    for edge in &dep_graph.edges {
        if edge.edge_type != EdgeType::Defines {
            continue;
        }
        
        match (&edge.from, &edge.to) {
            (NodeId::File(file_path), NodeId::Class(class_file, class_name)) => {
                let class_id = get_qualified_id(class_file, class_name);
                let mut m = HashMap::new();
                m.insert("file_path".to_string(), file_path.to_string());
                m.insert("class_id".to_string(), class_id);
                m.insert("repo_id".to_string(), repo_id.to_string());
                file_to_class.push(m);
            }
            (NodeId::File(file_path), NodeId::Function(func_file, func_name)) => {
                let func_id = get_qualified_id(func_file, func_name);
                let mut m = HashMap::new();
                m.insert("file_path".to_string(), file_path.to_string());
                m.insert("func_id".to_string(), func_id);
                m.insert("repo_id".to_string(), repo_id.to_string());
                file_to_func.push(m);
            }
            _ => {}
        }
    }

    // Batch File->Class DEFINES
    for chunk in file_to_class.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path, repo_id: edge.repo_id})
             MATCH (c:Class {id: edge.class_id, repo_id: edge.repo_id})
             MERGE (f)-[:DEFINES]->(c)"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert File->Class DEFINES")?;
    }

    // Batch File->Function DEFINES
    for chunk in file_to_func.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path, repo_id: edge.repo_id})
             MATCH (fn:Function {id: edge.func_id, repo_id: edge.repo_id})
             MERGE (f)-[:DEFINES]->(fn)"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert File->Function DEFINES")?;
    }
    
    info!("   Created {} DEFINES edges", file_to_class.len() + file_to_func.len());
    Ok(())
}

async fn batch_insert_contains_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    dep_graph: &DependencyGraph,
    batch_size: usize,
) -> Result<()> {
    let mut edges: Vec<BoltMap> = Vec::new();
    
    for edge in &dep_graph.edges {
        if edge.edge_type != EdgeType::Contains {
            continue;
        }
        
        if let (NodeId::Class(class_file, class_name), NodeId::Function(func_file, func_name)) = 
            (&edge.from, &edge.to) 
        {
            let class_id = get_qualified_id(class_file, class_name);
            let func_id = get_qualified_id(func_file, func_name);
            
            let mut m = HashMap::new();
            m.insert("class_id".to_string(), class_id);
            m.insert("func_id".to_string(), func_id);
            m.insert("repo_id".to_string(), repo_id.to_string());
            edges.push(m);
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (c:Class {id: edge.class_id, repo_id: edge.repo_id})
             MATCH (fn:Function {id: edge.func_id, repo_id: edge.repo_id})
             MERGE (c)-[:CONTAINS]->(fn)"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert CONTAINS edges")?;
    }
    
    info!("   Created {} CONTAINS edges", edges.len());
    Ok(())
}

async fn batch_insert_calls_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    dep_graph: &DependencyGraph,
    batch_size: usize,
) -> Result<()> {
    let mut edges: Vec<BoltMap> = Vec::new();
    
    for edge in &dep_graph.edges {
        if edge.edge_type != EdgeType::Calls {
            continue;
        }
        
        if let (NodeId::Function(from_file, from_name), NodeId::Function(to_file, to_name)) = 
            (&edge.from, &edge.to) 
        {
            let from_id = get_qualified_id(from_file, from_name);
            let to_id = get_qualified_id(to_file, to_name);
            
            let mut m = HashMap::new();
            m.insert("from_id".to_string(), from_id);
            m.insert("to_id".to_string(), to_id);
            m.insert("repo_id".to_string(), repo_id.to_string());
            edges.push(m);
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (from:Function {id: edge.from_id, repo_id: edge.repo_id})
             MATCH (to:Function {id: edge.to_id, repo_id: edge.repo_id})
             MERGE (from)-[:CALLS]->(to)"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert CALLS edges")?;
    }
    
    info!("   Created {} CALLS edges", edges.len());
    Ok(())
}

async fn batch_insert_imports_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    dep_graph: &DependencyGraph,
    batch_size: usize,
) -> Result<()> {
    let mut edges: Vec<BoltMap> = Vec::new();
    
    for edge in &dep_graph.edges {
        if edge.edge_type != EdgeType::Imports {
            continue;
        }
        
        if let (NodeId::File(file_path), NodeId::Module(module_name)) = (&edge.from, &edge.to) {
            let mut m = HashMap::new();
            m.insert("file_path".to_string(), file_path.to_string());
            m.insert("module_name".to_string(), module_name.to_string());
            m.insert("repo_id".to_string(), repo_id.to_string());
            edges.push(m);
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path, repo_id: edge.repo_id})
             MATCH (m:Module {name: edge.module_name, repo_id: edge.repo_id})
             MERGE (f)-[:IMPORTS]->(m)"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert IMPORTS edges")?;
    }
    
    info!("   Created {} IMPORTS edges", edges.len());
    Ok(())
}

async fn batch_insert_inherits_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    dep_graph: &DependencyGraph,
    batch_size: usize,
) -> Result<()> {
    let mut class_to_class: Vec<BoltMap> = Vec::new();
    let mut class_to_module: Vec<BoltMap> = Vec::new();
    
    for edge in &dep_graph.edges {
        if edge.edge_type != EdgeType::Inherits {
            continue;
        }
        
        let inheritance_type = edge
            .properties
            .get("kind")
            .cloned()
            .unwrap_or_else(|| "class".to_string());

        match (&edge.from, &edge.to) {
            (NodeId::Class(from_file, from_name), NodeId::Class(to_file, to_name)) => {
                let from_id = get_qualified_id(from_file, from_name);
                let to_id = get_qualified_id(to_file, to_name);
                
                let mut m = HashMap::new();
                m.insert("from_id".to_string(), from_id);
                m.insert("to_id".to_string(), to_id);
                m.insert("repo_id".to_string(), repo_id.to_string());
                m.insert("inheritance_type".to_string(), inheritance_type.clone());
                class_to_class.push(m);
            }
            (NodeId::Class(class_file, class_name), NodeId::Module(module_name)) => {
                let class_id = get_qualified_id(class_file, class_name);
                
                let mut m = HashMap::new();
                m.insert("class_id".to_string(), class_id);
                m.insert("module_name".to_string(), module_name.to_string());
                m.insert("repo_id".to_string(), repo_id.to_string());
                m.insert("inheritance_type".to_string(), inheritance_type.clone());
                class_to_module.push(m);
            }
            _ => {}
        }
    }

    // Batch Class->Class INHERITS
    for chunk in class_to_class.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (child:Class {id: edge.from_id, repo_id: edge.repo_id})
             MATCH (parent:Class {id: edge.to_id, repo_id: edge.repo_id})
               MERGE (child)-[r:INHERITS]->(parent)
               SET r.type = edge.inheritance_type"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert Class->Class INHERITS")?;
    }

    // Batch Class->Module INHERITS (external)
    for chunk in class_to_module.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (child:Class {id: edge.class_id, repo_id: edge.repo_id})
             MATCH (parent:Module {name: edge.module_name, repo_id: edge.repo_id})
               MERGE (child)-[r:INHERITS]->(parent)
               SET r.type = edge.inheritance_type"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert Class->Module INHERITS")?;
    }
    
    info!("   Created {} INHERITS edges", class_to_class.len() + class_to_module.len());
    Ok(())
}

// ============================================================================
// Boundary Nodes and Edges
// ============================================================================

async fn batch_insert_boundary_nodes(
    txn: &mut neo4rs::Txn,
    job_id: &str,
    repo_id: &str,
    boundary_result: &BoundaryDetectionResult,
    batch_size: usize,
) -> Result<()> {
    let nodes: Vec<HashMap<String, neo4rs::BoltType>> = boundary_result.boundaries
        .iter()
        .map(|b| {
            let mut m: HashMap<String, neo4rs::BoltType> = HashMap::new();
            m.insert("id".to_string(), b.id.clone().into());
            m.insert("name".to_string(), b.name.clone().into());
            m.insert("type".to_string(), b.boundary_type.as_str().to_string().into());
            m.insert("path".to_string(), b.path.clone().into());
            m.insert("job_id".to_string(), job_id.to_string().into());
            m.insert("repo_id".to_string(), repo_id.to_string().into());
            m.insert("file_count".to_string(), (b.file_count as i64).into());
            
            if let Some(layer) = &b.layer {
                m.insert("layer".to_string(), layer.as_str().to_string().into());
            }
            
            m
        })
        .collect();

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (b:Boundary {id: node.id})
             SET b.name = node.name,
                 b.type = node.type,
                 b.path = node.path,
                 b.job_id = node.job_id,
                 b.repo_id = node.repo_id,
                 b.file_count = node.file_count,
                 b.layer = COALESCE(node.layer, '')"
        )
        .param("nodes", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert boundary nodes")?;
    }
    
    info!("   Inserted {} Boundary nodes", nodes.len());
    Ok(())
}

async fn batch_insert_belongs_to_edges(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    boundary_result: &BoundaryDetectionResult,
    batch_size: usize,
) -> Result<()> {
    let mut edges = Vec::new();
    
    for boundary in &boundary_result.boundaries {
        for file_path in &boundary.files {
            let mut m = HashMap::new();
            m.insert("file_id".to_string(), file_path.clone());
            m.insert("boundary_id".to_string(), boundary.id.clone());
            m.insert("repo_id".to_string(), repo_id.to_string());
            edges.push(m);
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {id: edge.file_id, repo_id: edge.repo_id})
             MATCH (b:Boundary {id: edge.boundary_id, repo_id: edge.repo_id})
             MERGE (f)-[:BELONGS_TO]->(b)"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert BELONGS_TO edges")?;
    }
    
    info!("   Created {} BELONGS_TO edges", edges.len());
    Ok(())
}

/// Create file-to-file DEPENDS_ON edges based on import resolution
async fn batch_insert_file_dependencies(
    txn: &mut neo4rs::Txn,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    batch_size: usize,
) -> Result<()> {
    use std::path::Path;
    use std::collections::HashSet;
    
    // Build a map of module names to file paths for resolution
    let mut module_to_files: HashMap<String, Vec<String>> = HashMap::new();
    
    for file in parsed_files {
        let file_path = Path::new(&file.path);
        
        // Extract potential module names from file path
        // e.g., "src/utils/helper.ts" -> ["utils/helper", "helper"]
        if let Some(file_stem) = file_path.file_stem() {
            let stem_str = file_stem.to_string_lossy().to_string();
            module_to_files.entry(stem_str.clone()).or_default().push(file.path.clone());
            
            // Also add parent directory as potential module name
            if let Some(parent) = file_path.parent() {
                if let Some(parent_str) = parent.file_name() {
                    let parent_name = parent_str.to_string_lossy().to_string();
                    module_to_files.entry(parent_name).or_default().push(file.path.clone());
                }
            }
        }
    }
    
    // Now resolve imports to files
    let mut edges = Vec::new();
    let mut resolved_count = 0;
    
    for file in parsed_files {
        for import in &file.imports {
            // Try to resolve import to a file
            let mut resolved_files = HashSet::new();
            
            // Try exact match
            if module_to_files.contains_key(import) {
                resolved_files.extend(module_to_files.get(import).unwrap().clone());
            }
            
            // Try extracting last part of import path (e.g., "./utils/helper" -> "helper")
            if let Some(last_part) = import.split('/').last() {
                let clean_part = last_part.trim_start_matches("./").trim_start_matches("../");
                if module_to_files.contains_key(clean_part) {
                    resolved_files.extend(module_to_files.get(clean_part).unwrap().clone());
                }
            }
            
            // Try partial matches for relative imports
            if import.starts_with("./") || import.starts_with("../") {
                let import_parts: Vec<&str> = import.split('/').filter(|p| !p.is_empty() && *p != ".." && *p != ".").collect();
                if let Some(last_part) = import_parts.last() {
                    if module_to_files.contains_key(*last_part) {
                        resolved_files.extend(module_to_files.get(*last_part).unwrap().clone());
                    }
                }
            }
            
            // Create edges for resolved files (excluding self-imports)
            for target_file in resolved_files {
                if target_file != file.path {
                    let mut m = HashMap::new();
                    m.insert("source_file".to_string(), file.path.clone());
                    m.insert("target_file".to_string(), target_file);
                    m.insert("import_path".to_string(), import.clone());
                    m.insert("repo_id".to_string(), repo_id.to_string());
                    edges.push(m);
                    resolved_count += 1;
                }
            }
        }
    }
    
    // Batch insert edges
    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (source:File {path: edge.source_file, repo_id: edge.repo_id})
             MATCH (target:File {path: edge.target_file, repo_id: edge.repo_id})
             MERGE (source)-[d:DEPENDS_ON]->(target)
             ON CREATE SET d.import_path = edge.import_path"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert DEPENDS_ON edges")?;
    }
    
    info!("   Created {} DEPENDS_ON edges ({} imports resolved to files)", edges.len(), resolved_count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{FunctionInfo, ClassInfo};

    #[test]
    fn test_file_node_mapping_includes_repo_id() {
        let job_id = "job-123";
        let repo_id = "repo-456";
        let path = "src/main.rs";
        let language = "rust";

        let map = file_node_to_map(path, language, job_id, repo_id);

        assert_eq!(map.get("repo_id"), Some(&repo_id.to_string()));
        assert_eq!(map.get("job_id"), Some(&job_id.to_string()));
        assert_eq!(map.get("path"), Some(&path.to_string()));
        assert_eq!(map.get("id"), Some(&path.to_string()));
    }

    #[test]
    fn test_module_node_mapping_includes_repo_id() {
        let job_id = "job-123";
        let repo_id = "repo-456";
        let name = "my_module";

        let map = module_node_to_map(name, job_id, repo_id);

        assert_eq!(map.get("repo_id"), Some(&repo_id.to_string()));
        assert_eq!(map.get("job_id"), Some(&job_id.to_string()));
        assert_eq!(map.get("name"), Some(&name.to_string()));
    }

    // Since BoltType is complex to check equality on directly in HashMap, 
    // we verify keys exist and values are present (conceptually)
    // Note: BoltType doesn't implement Eq, so we can't easily assert_eq! on the map values directly
    // apart from String ones if converted. But we can check keys.
    #[test]
    fn test_function_node_keys_include_repo_id() {
        let job_id = "job-123";
        let repo_id = "repo-456";
        let file = "src/main.rs";
        
        let func = FunctionInfo {
            name: "my_func".to_string(),
            params: vec!["arg1".to_string()],
            return_type: Some("void".to_string()),
            calls: vec![],
            start_line: 10,
            end_line: 20,
        };

        let map = function_node_to_map(&func, file, job_id, repo_id);

        assert!(map.contains_key("repo_id"));
        assert!(map.contains_key("job_id"));
        assert!(map.contains_key("id"));
        assert!(map.contains_key("name"));
    }

    #[test]
    fn test_class_node_keys_include_repo_id() {
        let job_id = "job-123";
        let repo_id = "repo-456";
        let file = "src/main.rs";
        let name = "MyClass";

        let map = class_node_to_map(name, file, 10, 20, job_id, repo_id);

        assert!(map.contains_key("repo_id"));
        assert!(map.contains_key("job_id"));
        assert!(map.contains_key("id"));
    }
    #[test]
    fn test_qualified_id_generation() {
        let file = "src/main.rs";
        let name = "MyClass";
        // Verify format is file::name
        let expected = "src/main.rs::MyClass";
        
        assert_eq!(get_qualified_id(file, name), expected);
    }
}

