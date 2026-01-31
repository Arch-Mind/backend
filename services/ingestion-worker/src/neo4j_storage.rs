//! Neo4j Batch Storage
//!
//! Efficient batch storage for dependency graphs using UNWIND queries
//! and transaction support.

use crate::graph_builder::{DependencyGraph, EdgeType, NodeId};
use crate::parsers::{FunctionInfo, ParsedFile};
use anyhow::{Context, Result};
use neo4rs::query;
use std::collections::HashMap;
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

fn file_node_to_map(path: &str, language: &str, job_id: &str) -> BoltMap {
    let mut m = HashMap::new();
    m.insert("path".to_string(), path.to_string());
    m.insert("language".to_string(), language.to_string());
    m.insert("job_id".to_string(), job_id.to_string());
    m
}

fn class_node_to_map(name: &str, file: &str, start_line: usize, end_line: usize, job_id: &str) -> HashMap<String, neo4rs::BoltType> {
    let mut m: HashMap<String, neo4rs::BoltType> = HashMap::new();
    m.insert("name".to_string(), name.to_string().into());
    m.insert("file".to_string(), file.to_string().into());
    m.insert("start_line".to_string(), (start_line as i64).into());
    m.insert("end_line".to_string(), (end_line as i64).into());
    m.insert("job_id".to_string(), job_id.to_string().into());
    m
}

fn function_node_to_map(func: &FunctionInfo, file: &str, job_id: &str) -> HashMap<String, neo4rs::BoltType> {
    let mut m: HashMap<String, neo4rs::BoltType> = HashMap::new();
    m.insert("name".to_string(), func.name.clone().into());
    m.insert("file".to_string(), file.to_string().into());
    m.insert("start_line".to_string(), (func.start_line as i64).into());
    m.insert("end_line".to_string(), (func.end_line as i64).into());
    m.insert("params".to_string(), func.params.clone().into());
    m.insert("return_type".to_string(), func.return_type.clone().unwrap_or_default().into());
    m.insert("job_id".to_string(), job_id.to_string().into());
    m
}

fn module_node_to_map(name: &str, job_id: &str) -> BoltMap {
    let mut m = HashMap::new();
    m.insert("name".to_string(), name.to_string());
    m.insert("job_id".to_string(), job_id.to_string());
    m
}

fn edge_2_map(key1: &str, val1: &str, key2: &str, val2: &str) -> BoltMap {
    let mut m = HashMap::new();
    m.insert(key1.to_string(), val1.to_string());
    m.insert(key2.to_string(), val2.to_string());
    m
}

fn edge_4_map(k1: &str, v1: &str, k2: &str, v2: &str, k3: &str, v3: &str, k4: &str, v4: &str) -> BoltMap {
    let mut m = HashMap::new();
    m.insert(k1.to_string(), v1.to_string());
    m.insert(k2.to_string(), v2.to_string());
    m.insert(k3.to_string(), v3.to_string());
    m.insert(k4.to_string(), v4.to_string());
    m
}

fn edge_3_map(k1: &str, v1: &str, k2: &str, v2: &str, k3: &str, v3: &str) -> BoltMap {
    let mut m = HashMap::new();
    m.insert(k1.to_string(), v1.to_string());
    m.insert(k2.to_string(), v2.to_string());
    m.insert(k3.to_string(), v3.to_string());
    m
}

// ============================================================================
// Main Storage Function
// ============================================================================

/// Store the complete dependency graph in Neo4j using batch operations
pub async fn store_graph(
    graph_db: &neo4rs::Graph,
    job_id: &str,
    parsed_files: &[ParsedFile],
    dep_graph: &DependencyGraph,
    config: Option<BatchConfig>,
) -> Result<()> {
    let config = config.unwrap_or_default();
    info!("💾 Starting batch Neo4j storage (batch_size={})", config.batch_size);

    // Start a transaction
    let mut txn = graph_db.start_txn().await.context("Failed to start transaction")?;

    // Execute batch operations with error handling
    let result = execute_batch_operations(&mut txn, job_id, parsed_files, dep_graph, &config).await;

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
    parsed_files: &[ParsedFile],
    dep_graph: &DependencyGraph,
    config: &BatchConfig,
) -> Result<()> {
    // 1. Create Job node
    create_job_node(txn, job_id).await?;

    // 2. Batch insert nodes
    batch_insert_file_nodes(txn, job_id, parsed_files, config.batch_size).await?;
    batch_insert_class_nodes(txn, job_id, parsed_files, config.batch_size).await?;
    batch_insert_function_nodes(txn, job_id, parsed_files, config.batch_size).await?;
    batch_insert_module_nodes(txn, job_id, dep_graph, config.batch_size).await?;

    // 3. Batch insert edges
    batch_insert_defines_edges(txn, dep_graph, config.batch_size).await?;
    batch_insert_contains_edges(txn, dep_graph, config.batch_size).await?;
    batch_insert_calls_edges(txn, dep_graph, config.batch_size).await?;
    batch_insert_imports_edges(txn, dep_graph, config.batch_size).await?;
    batch_insert_inherits_edges(txn, dep_graph, config.batch_size).await?;

    Ok(())
}

// ============================================================================
// Job Node
// ============================================================================

async fn create_job_node(txn: &mut neo4rs::Txn, job_id: &str) -> Result<()> {
    let q = query(
        "CREATE (j:Job {id: $id, status: 'COMPLETED', timestamp: datetime()})"
    )
    .param("id", job_id);
    
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
    parsed_files: &[ParsedFile],
    batch_size: usize,
) -> Result<()> {
    let nodes: Vec<BoltMap> = parsed_files
        .iter()
        .map(|f| file_node_to_map(&f.path, &f.language, job_id))
        .collect();

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (f:File {path: node.path})
             SET f.language = node.language,
                 f.job_id = node.job_id"
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
    parsed_files: &[ParsedFile],
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<HashMap<String, neo4rs::BoltType>> = Vec::new();
    
    for file in parsed_files {
        for class in &file.classes {
            nodes.push(class_node_to_map(&class.name, &file.path, class.start_line, class.end_line, job_id));
        }
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (c:Class {name: node.name, file: node.file})
             SET c.start_line = node.start_line,
                 c.end_line = node.end_line,
                 c.job_id = node.job_id"
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
    parsed_files: &[ParsedFile],
    batch_size: usize,
) -> Result<()> {
    let mut nodes: Vec<HashMap<String, neo4rs::BoltType>> = Vec::new();
    
    for file in parsed_files {
        // Top-level functions
        for func in &file.functions {
            nodes.push(function_node_to_map(func, &file.path, job_id));
        }
        
        // Class methods
        for class in &file.classes {
            for method in &class.methods {
                nodes.push(function_node_to_map(method, &file.path, job_id));
            }
        }
    }

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (fn:Function {name: node.name, file: node.file})
             SET fn.start_line = node.start_line,
                 fn.end_line = node.end_line,
                 fn.params = node.params,
                 fn.return_type = node.return_type,
                 fn.job_id = node.job_id"
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
    dep_graph: &DependencyGraph,
    batch_size: usize,
) -> Result<()> {
    let nodes: Vec<BoltMap> = dep_graph
        .nodes
        .iter()
        .filter_map(|n| {
            if let NodeId::Module(name) = n {
                Some(module_node_to_map(name, job_id))
            } else {
                None
            }
        })
        .collect();

    for chunk in nodes.chunks(batch_size) {
        let q = query(
            "UNWIND $nodes AS node
             MERGE (m:Module {name: node.name})
             SET m.job_id = node.job_id"
        )
        .param("nodes", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert module nodes")?;
    }
    
    info!("   Inserted {} Module nodes", nodes.len());
    Ok(())
}

// ============================================================================
// Batch Edge Inserts
// ============================================================================

async fn batch_insert_defines_edges(
    txn: &mut neo4rs::Txn,
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
            (NodeId::File(file_path), NodeId::Class(_, class_name)) => {
                file_to_class.push(edge_2_map("file_path", file_path, "class_name", class_name));
            }
            (NodeId::File(file_path), NodeId::Function(_, func_name)) => {
                file_to_func.push(edge_2_map("file_path", file_path, "func_name", func_name));
            }
            _ => {}
        }
    }

    // Batch File->Class DEFINES
    for chunk in file_to_class.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path})
             MATCH (c:Class {name: edge.class_name, file: edge.file_path})
             MERGE (f)-[:DEFINES]->(c)"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert File->Class DEFINES")?;
    }

    // Batch File->Function DEFINES
    for chunk in file_to_func.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path})
             MATCH (fn:Function {name: edge.func_name, file: edge.file_path})
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
            edges.push(edge_4_map(
                "class_file", class_file, 
                "class_name", class_name,
                "func_file", func_file,
                "func_name", func_name
            ));
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (c:Class {name: edge.class_name, file: edge.class_file})
             MATCH (fn:Function {name: edge.func_name, file: edge.func_file})
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
            edges.push(edge_4_map(
                "from_file", from_file,
                "from_name", from_name,
                "to_file", to_file,
                "to_name", to_name
            ));
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (from:Function {name: edge.from_name, file: edge.from_file})
             MATCH (to:Function {name: edge.to_name, file: edge.to_file})
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
    dep_graph: &DependencyGraph,
    batch_size: usize,
) -> Result<()> {
    let mut edges: Vec<BoltMap> = Vec::new();
    
    for edge in &dep_graph.edges {
        if edge.edge_type != EdgeType::Imports {
            continue;
        }
        
        if let (NodeId::File(file_path), NodeId::Module(module_name)) = (&edge.from, &edge.to) {
            edges.push(edge_2_map("file_path", file_path, "module_name", module_name));
        }
    }

    for chunk in edges.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (f:File {path: edge.file_path})
             MATCH (m:Module {name: edge.module_name})
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
    dep_graph: &DependencyGraph,
    batch_size: usize,
) -> Result<()> {
    let mut class_to_class: Vec<BoltMap> = Vec::new();
    let mut class_to_module: Vec<BoltMap> = Vec::new();
    
    for edge in &dep_graph.edges {
        if edge.edge_type != EdgeType::Inherits {
            continue;
        }
        
        match (&edge.from, &edge.to) {
            (NodeId::Class(from_file, from_name), NodeId::Class(to_file, to_name)) => {
                class_to_class.push(edge_4_map(
                    "from_file", from_file,
                    "from_name", from_name,
                    "to_file", to_file,
                    "to_name", to_name
                ));
            }
            (NodeId::Class(class_file, class_name), NodeId::Module(module_name)) => {
                class_to_module.push(edge_3_map(
                    "class_file", class_file,
                    "class_name", class_name,
                    "module_name", module_name
                ));
            }
            _ => {}
        }
    }

    // Batch Class->Class INHERITS
    for chunk in class_to_class.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (child:Class {name: edge.from_name, file: edge.from_file})
             MATCH (parent:Class {name: edge.to_name, file: edge.to_file})
             MERGE (child)-[:INHERITS]->(parent)"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert Class->Class INHERITS")?;
    }

    // Batch Class->Module INHERITS (external)
    for chunk in class_to_module.chunks(batch_size) {
        let q = query(
            "UNWIND $edges AS edge
             MATCH (child:Class {name: edge.class_name, file: edge.class_file})
             MATCH (parent:Module {name: edge.module_name})
             MERGE (child)-[:INHERITS]->(parent)"
        )
        .param("edges", chunk.to_vec());
        
        txn.run(q).await.context("Failed to batch insert Class->Module INHERITS")?;
    }
    
    info!("   Created {} INHERITS edges", class_to_class.len() + class_to_module.len());
    Ok(())
}
