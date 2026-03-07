import re

with open('src/neo4j_storage.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Add macro at the top
macro = '''
macro_rules! retry_query {
    (, ) => {{
        let max_retries = 3;
        let mut attempt = 0;
        let mut last_err = anyhow::anyhow!("Unknown error");
        loop {
            attempt += 1;
            let mut txn = match .start_txn().await {
                Ok(t) => t,
                Err(e) => {
                    if attempt >= max_retries {
                        last_err = e.into();
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << (attempt - 1)))).await;
                    continue;
                }
            };
            
            match txn.run().await {
                Ok(_) => {
                    match txn.commit().await {
                        Ok(_) => break Ok(()),
                        Err(e) => {
                            if attempt >= max_retries {
                                last_err = e.into();
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << (attempt - 1)))).await;
                        }
                    }
                }
                Err(e) => {
                    let _ = txn.rollback().await;
                    if attempt >= max_retries {
                        last_err = e.into();
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << (attempt - 1)))).await;
                }
            }
        }
        if attempt >= max_retries {
            Err(last_err)
        } else {
            Ok(())
        }
    }};
}
'''
content = content.replace('use tracing::{info, warn};', 'use tracing::{info, warn};\n' + macro)

# 2. Replace txn to graph_db everywhere
content = re.sub(r'txn: &mut neo4rs::Txn', 'graph_db: &neo4rs::Graph', content)

# 3. Replace txn.run with retry_query
pattern = r'(\s*)let\s+q\s*=\s*(query\([\s\S]*?\)\s*\.param\([\s\S]*?;\s*)txn\.run\(q\)\.await'

def replace_query(match):
    indent = match.group(1)
    q_body = match.group(2)
    q_body = q_body.strip()
    if q_body.endswith(';'):
        q_body = q_body[:-1]
    return f'{indent}retry_query!(graph_db, {{\n{indent}    {q_body}\n{indent}}})'

content = re.sub(pattern, replace_query, content)

# also replace non-chained txn.run
pattern2 = r'(\s*)let\s+remove_[\w]+\s*=\s*(query\([\s\S]*?\)\s*\.param\([\s\S]*?;\s*)txn\.run\([^)]+\)\s*\.await'
content = re.sub(pattern2, replace_query, content)

# create_job_node
pattern3 = r'(\s*)let\s+q\s*=\s*(query\([\s\S]*?\)\s*\.param\([\s\S]*?;\s*)txn\.run\(q\)\.await'
content = re.sub(pattern3, replace_query, content)

# 4. Replace store_graph completely
sg_pattern = r'pub async fn store_graph\([\s\S]*?\}\s*\}\s*\}\s*\}'
new_sg = '''pub async fn store_graph(
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
    progress_tx: Option<tokio::sync::mpsc::Sender<i32>>,
) -> Result<()> {
    let config = config.unwrap_or_default();
    execute_batch_operations(
        graph_db, 
        job_id, 
        repo_id, 
        parsed_files, 
        dep_graph, 
        git_contributions,
        boundary_result,
        library_dependencies,
        communication_analysis,
        &config,
        progress_tx
    ).await
}'''
content = re.sub(sg_pattern, new_sg, content, count=1)

# 5. Replace store_graph_incremental completely
# Use string index to find where n store_graph_incremental starts and ends.
start_idx = content.find('pub async fn store_graph_incremental')
if start_idx != -1:
    end_idx = content.find('// =================================', start_idx)
    new_sgi = '''pub async fn store_graph_incremental(
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
    progress_tx: Option<tokio::sync::mpsc::Sender<i32>>,
) -> Result<()> {
    let config = config.unwrap_or_default();
    let mut files_to_remove = Vec::new();
    files_to_remove.extend_from_slice(changed_files);
    files_to_remove.extend_from_slice(removed_files);
    files_to_remove.sort();
    files_to_remove.dedup();

    delete_file_nodes(graph_db, repo_id, &files_to_remove).await?;

    execute_batch_operations(
        graph_db,
        job_id,
        repo_id,
        parsed_files,
        dep_graph,
        git_contributions,
        boundary_result,
        library_dependencies,
        communication_analysis,
        &config,
        progress_tx
    )
    .await
}
'''
    content = content[:start_idx] + new_sgi + content[end_idx:]

# 6. Replace execute_batch_operations to update progress
content = content.replace(
'''async fn execute_batch_operations(
    graph_db: &neo4rs::Graph,
    job_id: &str,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    dep_graph: &DependencyGraph,
    git_contributions: Option<&RepoContributions>,
    boundary_result: &BoundaryDetectionResult,
    library_dependencies: &[LibraryDependency],
    communication_analysis: &CommunicationAnalysis,
    config: &BatchConfig,
) -> Result<()> {''',
'''async fn execute_batch_operations(
    graph_db: &neo4rs::Graph,
    job_id: &str,
    repo_id: &str,
    parsed_files: &[ParsedFile],
    dep_graph: &DependencyGraph,
    git_contributions: Option<&RepoContributions>,
    boundary_result: &BoundaryDetectionResult,
    library_dependencies: &[LibraryDependency],
    communication_analysis: &CommunicationAnalysis,
    config: &BatchConfig,
    progress_tx: Option<tokio::sync::mpsc::Sender<i32>>,
) -> Result<()> {
    let update_prog = |p: i32| {
        if let Some(tx) = &progress_tx {
            let _ = tx.try_send(p);
        }
    };
''')

# insert update_prog(X) between function calls

replacements = [
    ('batch_insert_class_nodes(graph_db', 'update_prog(76);\n    batch_insert_class_nodes(graph_db'),
    ('batch_insert_function_nodes(graph_db', 'update_prog(77);\n    batch_insert_function_nodes(graph_db'),
    ('batch_insert_module_nodes(graph_db', 'update_prog(78);\n    batch_insert_module_nodes(graph_db'),
    ('batch_insert_boundary_nodes(graph_db', 'update_prog(79);\n    batch_insert_boundary_nodes(graph_db'),
    ('batch_insert_library_nodes(graph_db', 'update_prog(80);\n    batch_insert_library_nodes(graph_db'),
    ('batch_insert_defines_edges(graph_db', 'update_prog(81);\n    batch_insert_defines_edges(graph_db'),
    ('batch_insert_contains_edges(graph_db', 'update_prog(82);\n    batch_insert_contains_edges(graph_db'),
    ('batch_insert_calls_edges(graph_db', 'update_prog(83);\n    batch_insert_calls_edges(graph_db'),
    ('batch_insert_imports_edges(graph_db', 'update_prog(84);\n    batch_insert_imports_edges(graph_db'),
    ('batch_insert_inherits_edges(graph_db', 'update_prog(85);\n    batch_insert_inherits_edges(graph_db'),
    ('batch_insert_belongs_to_edges(graph_db', 'update_prog(86);\n    batch_insert_belongs_to_edges(graph_db'),
    ('batch_insert_library_edges(graph_db', 'update_prog(87);\n    batch_insert_library_edges(graph_db'),
    ('batch_insert_endpoint_nodes(graph_db', 'update_prog(88);\n    batch_insert_endpoint_nodes(graph_db'),
    ('batch_insert_file_dependencies(graph_db', 'update_prog(89);\n    batch_insert_file_dependencies(graph_db')
]
for (old, new) in replacements:
    content = content.replace(old, new)

with open('src/neo4j_storage.rs', 'w', encoding='utf-8') as f:
    f.write(content)
print("Done refactoring neo4j_storage.rs")
