import os
import re

def main():
    path = r'c:\Users\slikh\Documents\Archmind\backend\services\ingestion-worker\src\neo4j_storage.rs'
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    # 1. Macro insertion
    macro_code = """
macro_rules! execute_with_retry {
    ($graph:expr, $query_block:expr, $max_retries:expr) => {{
        let mut attempt = 0;
        let mut final_result: Result<(), anyhow::Error> = Err(anyhow::anyhow!("Retry loop failed"));
        loop {
            attempt += 1;
            let mut txn = match $graph.start_txn().await {
                Ok(t) => t,
                Err(e) => {
                    if attempt >= $max_retries {
                        final_result = Err(anyhow::anyhow!("Failed to start transaction: {}", e));
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(500 * (1 << (attempt - 1)))).await;
                    continue;
                }
            };
            
            let q = $query_block; 
            
            match txn.run(q).await {
                Ok(_) => match txn.commit().await {
                    Ok(_) => { final_result = Ok(()); break; },
                    Err(e) => {
                        if attempt >= $max_retries {
                            final_result = Err(anyhow::anyhow!("Failed to commit batch: {}", e));
                            break;
                        }
                    }
                },
                Err(e) => {
                    let _ = txn.rollback().await;
                    if attempt >= $max_retries {
                        final_result = Err(anyhow::anyhow!("Query failed: {}", e));
                        break;
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500 * (1 << (attempt - 1)))).await;
        }
        final_result
    }};
}

// ============================================================================
// Configuration
"""
    if "macro_rules! execute_with_retry" not in content:
        content = content.replace("// ============================================================================\n// Configuration", macro_code, 1)

    # 2. Change signatures
    content = content.replace("txn: &mut neo4rs::Txn", "graph: &neo4rs::Graph")
    
    # 3. Handle delete_file_nodes internally
    content = content.replace("let remove_files = query", "let remove_files = || query")
    content = content.replace("txn.run(remove_files)\n        .await\n        .context(\"Failed to delete file nodes\")?;", 
                              "execute_with_retry!(graph, remove_files(), 3).context(\"Failed to delete file nodes\")?;")

    content = content.replace("let remove_classes = query", "let remove_classes = || query")
    content = content.replace("txn.run(remove_classes)\n        .await\n        .context(\"Failed to delete class nodes\")?;", 
                              "execute_with_retry!(graph, remove_classes(), 3).context(\"Failed to delete class nodes\")?;")

    content = content.replace("let remove_functions = query", "let remove_functions = || query")
    content = content.replace("txn.run(remove_functions)\n        .await\n        .context(\"Failed to delete function nodes\")?;", 
                              "execute_with_retry!(graph, remove_functions(), 3).context(\"Failed to delete function nodes\")?;")
                              
    # 4. Handle create_job_node
    content = content.replace("let q = query(\n        \"MERGE (j:Job {id: $id, repo_id: $repo_id})\n         SET j.status = 'COMPLETED', j.timestamp = datetime()\"\n    )\n    .param(\"id\", job_id)\n    .param(\"repo_id\", repo_id);\n    \n    txn.run(q).await.context(\"Failed to create job node\")?;",
                              "execute_with_retry!(graph,\n        query(\"MERGE (j:Job {id: $id, repo_id: $repo_id}) SET j.status = 'COMPLETED', j.timestamp = datetime()\")\n        .param(\"id\", job_id)\n        .param(\"repo_id\", repo_id),\n        3\n    ).context(\"Failed to create job node\")?;")

    # 5. Handle all the loop queries
    # Instead of doing massive regexes, let's just do exact string replacements targeting `let q = query(` to `let q_factory = || query(`
    # Oh wait, with macro we don't need `|| query`. We can literally just do:
    # let q = query(...)...; txn.run(q).await
    # Since we need to inline it: 
    # execute_with_retry!(graph, query(...)..., 3)
    
    # Regex approach:
    # Match:
    # let q = <anything ending with ;>
    # txn.run(q).await.context(<context>)?;
    
    pattern = re.compile(r'let q = (query\(.*?\).param\(.*?\));\s*txn\.run\(q\)\.await\.context\((.*?)\)\?;', re.DOTALL)
    
    def repl(m):
        q_expr = m.group(1).strip()
        context_msg = m.group(2)
        return f'execute_with_retry!(graph, {q_expr}, 3).context({context_msg})?;'
    
    content = pattern.sub(repl, content)

    # 6. We also need to rewrite store_graph and store_graph_incremental because they used `txn` themselves.
    # Since these form the entrypoints, we strip their big loops and just pass `graph_db` to `execute_batch_operations`.
    
    # We will just manually fix store_graph and store_graph_incremental in another step, or overwrite them here.

    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

if __name__ == "__main__":
    main()
