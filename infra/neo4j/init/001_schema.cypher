// Neo4j Database Initialization Script for ArchMind
// Run this script in Neo4j Browser or cypher-shell

// ==================== Create Constraints ====================

// Ensure unique identifiers for nodes
CREATE CONSTRAINT file_id IF NOT EXISTS
FOR (f:File) REQUIRE f.id IS UNIQUE;

CREATE CONSTRAINT function_id IF NOT EXISTS
FOR (fn:Function) REQUIRE fn.id IS UNIQUE;

CREATE CONSTRAINT class_id IF NOT EXISTS
FOR (c:Class) REQUIRE c.id IS UNIQUE;

CREATE CONSTRAINT module_id IF NOT EXISTS
FOR (m:Module) REQUIRE m.id IS UNIQUE;

CREATE CONSTRAINT job_id IF NOT EXISTS
FOR (j:Job) REQUIRE j.id IS UNIQUE;

CREATE CONSTRAINT repository_id IF NOT EXISTS
FOR (r:Repository) REQUIRE r.id IS UNIQUE;

// ==================== Create Indexes ====================

// Improve query performance
CREATE INDEX file_repo_id IF NOT EXISTS
FOR (f:File) ON (f.repo_id);

CREATE INDEX function_repo_id IF NOT EXISTS
FOR (fn:Function) ON (fn.repo_id);

CREATE INDEX class_repo_id IF NOT EXISTS
FOR (c:Class) ON (c.repo_id);

CREATE INDEX module_repo_id IF NOT EXISTS
FOR (m:Module) ON (m.repo_id);

CREATE INDEX file_path IF NOT EXISTS
FOR (f:File) ON (f.path);

CREATE INDEX function_name IF NOT EXISTS
FOR (fn:Function) ON (fn.name);

CREATE INDEX class_name IF NOT EXISTS
FOR (c:Class) ON (c.name);

// ==================== Sample Data (Optional) ====================

// Create a sample repository node
CREATE (r:Repository {
    id: 'sample-repo',
    name: 'Sample Repository',
    url: 'https://github.com/example/sample',
    created_at: datetime()
});

// Create sample file nodes
CREATE (f1:File {
    id: 'file-main',
    repo_id: 'sample-repo',
    path: 'src/main.rs',
    language: 'rust',
    created_at: datetime()
});

CREATE (f2:File {
    id: 'file-lib',
    repo_id: 'sample-repo',
    path: 'src/lib.rs',
    language: 'rust',
    created_at: datetime()
});

// Create sample function nodes
CREATE (fn1:Function {
    id: 'function-main',
    repo_id: 'sample-repo',
    name: 'main',
    signature: 'fn main()',
    file_path: 'src/main.rs',
    line_start: 1,
    line_end: 10,
    created_at: datetime()
});

CREATE (fn2:Function {
    id: 'function-process',
    repo_id: 'sample-repo',
    name: 'process',
    signature: 'fn process(data: String) -> Result<()>',
    file_path: 'src/lib.rs',
    line_start: 20,
    line_end: 50,
    created_at: datetime()
});

// Create sample class node
CREATE (c1:Class {
    id: 'class-parser',
    repo_id: 'sample-repo',
    name: 'Parser',
    type: 'struct',
    file_path: 'src/lib.rs',
    line_start: 60,
    line_end: 100,
    created_at: datetime()
});

// ==================== Create Relationships ====================

// File contains functions
MATCH (f:File {id: 'file-main'}), (fn:Function {id: 'function-main'})
CREATE (f)-[:CONTAINS]->(fn);

MATCH (f:File {id: 'file-lib'}), (fn:Function {id: 'function-process'})
CREATE (f)-[:CONTAINS]->(fn);

MATCH (f:File {id: 'file-lib'}), (c:Class {id: 'class-parser'})
CREATE (f)-[:CONTAINS]->(c);

// Function calls another function
MATCH (fn1:Function {id: 'function-main'}), (fn2:Function {id: 'function-process'})
CREATE (fn1)-[:CALLS {line: 5}]->(fn2);

// File imports another file
MATCH (f1:File {id: 'file-main'}), (f2:File {id: 'file-lib'})
CREATE (f1)-[:IMPORTS]->(f2);

// Repository contains files
MATCH (r:Repository {id: 'sample-repo'}), (f:File {repo_id: 'sample-repo'})
CREATE (r)-[:CONTAINS]->(f);

// ==================== Verification Queries ====================

// Count nodes by type
MATCH (n)
RETURN labels(n)[0] AS NodeType, count(n) AS Count
ORDER BY Count DESC;

// Count relationships by type
MATCH ()-[r]->()
RETURN type(r) AS RelationshipType, count(r) AS Count
ORDER BY Count DESC;

// Show sample repository structure
MATCH (r:Repository {id: 'sample-repo'})-[:CONTAINS]->(f:File)
OPTIONAL MATCH (f)-[:CONTAINS]->(child)
RETURN r.name AS Repository, 
       f.path AS File, 
       labels(child)[0] AS ChildType,
       child.name AS ChildName
ORDER BY f.path, ChildType, ChildName;

// ==================== Useful Queries for Development ====================

// Find all files in a repository
// MATCH (f:File {repo_id: $repo_id})
// RETURN f.path, f.language, f.id
// ORDER BY f.path;

// Find all functions that call a specific function
// MATCH (caller:Function)-[:CALLS]->(target:Function {id: $function_id})
// RETURN caller.name, caller.file_path;

// Find impact analysis (what depends on a node)
// MATCH path = (n {id: $node_id})-[:CALLS|IMPORTS|INHERITS*1..3]-(impacted)
// RETURN DISTINCT impacted.id, impacted.name, length(path) AS distance
// ORDER BY distance;

// Find all imports of a file
// MATCH (f:File {id: $file_id})-[:IMPORTS]->(imported:File)
// RETURN imported.path, imported.language;

// Find circular dependencies
// MATCH path = (f1:File)-[:IMPORTS*2..5]->(f1)
// RETURN [node IN nodes(path) | node.path] AS CircularPath;

// ==================== Completed ====================
RETURN 'ArchMind Neo4j schema initialized successfully!' AS status;
