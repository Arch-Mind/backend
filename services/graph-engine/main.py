from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import Dict, List, Optional
import os
from dotenv import load_dotenv
from neo4j import GraphDatabase
import networkx as nx
import logging

# Load environment variables
load_dotenv()

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Initialize FastAPI app
app = FastAPI(
    title="ArchMind Graph Engine",
    description="Graph intelligence and analysis service",
    version="0.1.0"
)

# CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Neo4j connection
neo4j_uri = os.getenv("NEO4J_URI", "bolt://localhost:7687")
neo4j_user = os.getenv("NEO4J_USER", "neo4j")
neo4j_password = os.getenv("NEO4J_PASSWORD", "password")


def connect_neo4j_with_retry(uri: str, user: str, password: str, max_retries: int = 4):
    """
    Connect to Neo4j with exponential backoff retry logic.
    
    Args:
        uri: Neo4j connection URI
        user: Username for authentication
        password: Password for authentication
        max_retries: Maximum number of retry attempts (default: 4)
    
    Returns:
        Neo4j driver instance or None if all retries fail
    """
    import time
    
    for attempt in range(1, max_retries + 1):
        try:
            logger.info(f"🔄 Attempting to connect to Neo4j at {uri}... (attempt {attempt}/{max_retries})")
            driver = GraphDatabase.driver(uri, auth=(user, password))
            # Verify connectivity
            driver.verify_connectivity()
            logger.info(f"✅ Successfully connected to Neo4j at {uri}")
            return driver
        except Exception as e:
            if attempt < max_retries:
                wait_time = 2 ** (attempt - 1)  # Exponential backoff: 1s, 2s, 4s, 8s
                logger.warning(
                    f"⚠️  Failed to connect to Neo4j: {e}. "
                    f"Retrying in {wait_time}s (attempt {attempt}/{max_retries})..."
                )
                time.sleep(wait_time)
            else:
                logger.error(f"❌ Failed to connect to Neo4j after {max_retries} attempts: {e}")
                return None


# Initialize Neo4j connection with retry
neo4j_driver = connect_neo4j_with_retry(neo4j_uri, neo4j_user, neo4j_password)


# Pydantic models
class ImpactAnalysisRequest(BaseModel):
    node_id: str
    depth: int = 3


class MetricsResponse(BaseModel):
    total_files: int
    total_functions: int
    total_classes: int
    total_dependencies: int
    complexity_score: float


class GraphNode(BaseModel):
    id: str
    label: str
    type: str
    properties: Dict


class GraphEdge(BaseModel):
    source: str
    target: str
    type: str


class GraphResponse(BaseModel):
    nodes: List[GraphNode]
    edges: List[GraphEdge]
    total_nodes: Optional[int] = None
    total_edges: Optional[int] = None


class PaginatedGraphResponse(BaseModel):
    nodes: List[GraphNode]
    edges: List[GraphEdge]
    total_nodes: int
    total_edges: int
    limit: int
    offset: int
    has_more: bool


class ErrorResponse(BaseModel):
    error: str
    details: Optional[str] = None
    repo_id: Optional[str] = None


# Helper functions
def validate_repo_id(repo_id: str) -> bool:
    """Validate repo_id format (UUID)."""
    import re
    uuid_pattern = r'^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$'
    return bool(re.match(uuid_pattern, repo_id, re.I))


async def check_repo_exists(session, repo_id: str) -> bool:
    """Check if repo_id or job_id exists in the database."""
    try:
        # Check for both repo_id and job_id to support both identifiers
        result = session.run(
            "MATCH (n) WHERE n.repo_id = $repo_id OR n.job_id = $repo_id RETURN count(n) as count LIMIT 1",
            repo_id=repo_id
        )
        record = result.single()
        return record and record["count"] > 0
    except Exception as e:
        logger.error(f"Error checking repo existence: {e}")
        return False


async def get_total_count(session, query: str, repo_id: str) -> int:
    """Get total count for pagination."""
    try:
        result = session.run(query, repo_id=repo_id)
        record = result.single()
        return record["count"] if record else 0
    except Exception:
        return 0


def validate_pagination_params(limit: int, offset: int) -> tuple:
    """Validate and normalize pagination parameters."""
    limit = max(1, min(limit, 1000))  # Clamp between 1 and 1000
    offset = max(0, offset)  # Non-negative
    return limit, offset


# Routes
@app.get("/")
async def root():
    return {
        "service": "ArchMind Graph Engine",
        "version": "0.1.0",
        "status": "running"
    }


@app.get("/health")
async def health_check():
    neo4j_status = "healthy"
    if neo4j_driver:
        try:
            neo4j_driver.verify_connectivity()
        except Exception:
            neo4j_status = "unhealthy"
    else:
        neo4j_status = "disconnected"

    return {
        "status": "ok",
        "services": {
            "neo4j": neo4j_status
        }
    }


@app.get("/api/impact/{node_id}")
async def get_impact_analysis(node_id: str, depth: int = 3):
    """
    Analyze the impact of changes to a specific node.
    Returns all nodes that would be affected if this node changes.
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")

    try:
        with neo4j_driver.session() as session:
            # Query to find all nodes impacted by changes to the given node
            # This follows CALLS, IMPORTS, and INHERITS relationships
            query = """
            MATCH path = (n {id: $node_id})-[:CALLS|IMPORTS|INHERITS*1..3]-(impacted)
            RETURN DISTINCT impacted.id as id, 
                   impacted.name as name, 
                   labels(impacted)[0] as type,
                   length(path) as distance
            ORDER BY distance
            LIMIT 100
            """
            
            result = session.run(query, node_id=node_id)
            impacted_nodes = [
                {
                    "id": record["id"],
                    "name": record["name"],
                    "type": record["type"],
                    "distance": record["distance"]
                }
                for record in result
            ]

            return {
                "node_id": node_id,
                "impacted_count": len(impacted_nodes),
                "impacted_nodes": impacted_nodes
            }
    except Exception as e:
        logger.error(f"Impact analysis error: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/api/metrics/{repo_id}")
async def get_repository_metrics(repo_id: str):
    """
    Calculate and return metrics for a repository.
    repo_id is the job_id from the analysis job.
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")

    # Validate repo_id format
    if not validate_repo_id(repo_id):
        raise HTTPException(
            status_code=400, 
            detail=f"Invalid repo_id format. Expected UUID, got: {repo_id}"
        )

    try:
        with neo4j_driver.session() as session:
            # Check if repo exists
            repo_exists = await check_repo_exists(session, repo_id)
            if not repo_exists:
                raise HTTPException(
                    status_code=404,
                    detail=f"Repository not found: {repo_id}. Please ensure the analysis job has completed successfully."
                )

            # Count files - using repo_id or job_id property
            files_result = session.run(
                "MATCH (f:File) WHERE f.repo_id = $repo_id OR f.job_id = $repo_id RETURN count(f) as count",
                repo_id=repo_id
            )
            files_record = files_result.single()
            total_files = files_record["count"] if files_record else 0

            # Count functions
            functions_result = session.run(
                "MATCH (fn:Function) WHERE fn.repo_id = $repo_id OR fn.job_id = $repo_id RETURN count(fn) as count",
                repo_id=repo_id
            )
            functions_record = functions_result.single()
            total_functions = functions_record["count"] if functions_record else 0

            # Count classes
            classes_result = session.run(
                "MATCH (c:Class) WHERE c.repo_id = $repo_id OR c.job_id = $repo_id RETURN count(c) as count",
                repo_id=repo_id
            )
            classes_record = classes_result.single()
            total_classes = classes_record["count"] if classes_record else 0

            # Count dependencies (edges don't have job_id, count by matching nodes)
            deps_result = session.run(
                "MATCH (a)-[r:CALLS|IMPORTS|INHERITS]->(b) WHERE (a.repo_id = $repo_id OR a.job_id = $repo_id) AND (b.repo_id = $repo_id OR b.job_id = $repo_id) RETURN count(r) as count",
                repo_id=repo_id
            )
            deps_record = deps_result.single()
            total_dependencies = deps_record["count"] if deps_record else 0

            # Calculate complexity score (simplified)
            complexity_score = (total_dependencies / max(total_functions, 1)) * 10

            return MetricsResponse(
                total_files=total_files,
                total_functions=total_functions,
                total_classes=total_classes,
                total_dependencies=total_dependencies,
                complexity_score=round(complexity_score, 2)
            )
    except HTTPException:
        # Re-raise HTTP exceptions
        raise
    except Exception as e:
        logger.error(f"Metrics calculation error for {repo_id}: {e}")
        raise HTTPException(status_code=500, detail=f"Internal server error: {str(e)}")


@app.get("/api/graph/{repo_id}")
async def get_dependency_graph(repo_id: str, limit: int = 100, offset: int = 0):
    """
    Retrieve the full dependency graph for a repository with pagination.
    repo_id is the job_id from the analysis job.
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")

    # Validate repo_id format
    if not validate_repo_id(repo_id):
        raise HTTPException(
            status_code=400,
            detail=f"Invalid repo_id format. Expected UUID, got: {repo_id}"
        )

    # Validate and normalize pagination parameters
    limit, offset = validate_pagination_params(limit, offset)

    try:
        with neo4j_driver.session() as session:
            # Check if repo exists
            repo_exists = await check_repo_exists(session, repo_id)
            if not repo_exists:
                raise HTTPException(
                    status_code=404,
                    detail=f"Repository not found: {repo_id}. Please ensure the analysis job has completed successfully."
                )

            # Get total count of nodes
            total_nodes_query = "MATCH (n) WHERE n.repo_id = $repo_id OR n.job_id = $repo_id RETURN count(n) as count"
            total_nodes = await get_total_count(session, total_nodes_query, repo_id)

            # Get total count of edges
            total_edges_query = "MATCH (a)-[r]->(b) WHERE (a.repo_id = $repo_id OR a.job_id = $repo_id) AND (b.repo_id = $repo_id OR b.job_id = $repo_id) RETURN count(r) as count"
            total_edges = await get_total_count(session, total_edges_query, repo_id)

            # Get nodes with pagination
            nodes_query = """
            MATCH (n)
            WHERE n.repo_id = $repo_id OR n.job_id = $repo_id
            RETURN 
                COALESCE(n.path, n.name, n.id, toString(id(n))) as id,
                COALESCE(n.name, n.path, toString(id(n))) as name,
                labels(n)[0] as type,
                properties(n) as props
            SKIP $offset
            LIMIT $limit
            """
            nodes_result = session.run(nodes_query, repo_id=repo_id, limit=limit, offset=offset)
            nodes = []
            for record in nodes_result:
                try:
                    node_id = str(record["id"]) if record["id"] else f"node_{len(nodes)}"
                    node_name = str(record["name"]) if record["name"] else node_id
                    # For files, extract just the filename from the path
                    if record["type"] == "File" and "/" in node_name:
                        node_name = node_name.split("/")[-1]
                    elif record["type"] == "File" and "\\" in node_name:
                        node_name = node_name.split("\\")[-1]
                    
                    node = GraphNode(
                        id=node_id,
                        label=node_name,
                        type=record["type"] or "Unknown",
                        properties=record["props"] or {}
                    )
                    nodes.append(node)
                except Exception as e:
                    logger.warning(f"Skipping invalid node: {e}")

            # Get edges with pagination
            edges_query = """
            MATCH (a)-[r]->(b)
            WHERE (a.repo_id = $repo_id OR a.job_id = $repo_id) AND (b.repo_id = $repo_id OR b.job_id = $repo_id)
            RETURN 
                COALESCE(a.path, a.name, a.id, toString(id(a))) as source,
                COALESCE(b.path, b.name, b.id, toString(id(b))) as target,
                type(r) as type
            SKIP $offset
            LIMIT $limit
            """
            edges_result = session.run(edges_query, repo_id=repo_id, limit=limit, offset=offset)
            edges = []
            for record in edges_result:
                try:
                    source = str(record["source"]) if record["source"] else None
                    target = str(record["target"]) if record["target"] else None
                    if source and target:
                        edge = GraphEdge(
                            source=source,
                            target=target,
                            type=record["type"] or "UNKNOWN"
                        )
                        edges.append(edge)
                except Exception as e:
                    logger.warning(f"Skipping invalid edge: {e}")

            # Check if there are more results
            has_more = (offset + limit) < max(total_nodes, total_edges)

            return PaginatedGraphResponse(
                nodes=nodes,
                edges=edges,
                total_nodes=total_nodes,
                total_edges=total_edges,
                limit=limit,
                offset=offset,
                has_more=has_more
            )
    except HTTPException:
        # Re-raise HTTP exceptions
        raise
    except Exception as e:
        logger.error(f"Graph retrieval error for {repo_id}: {e}")
        raise HTTPException(status_code=500, detail=f"Internal server error: {str(e)}")


@app.post("/api/query")
async def execute_cypher_query(query: str, params: Optional[Dict] = None):
    """
    Execute a custom Cypher query (use with caution).
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")

    try:
        with neo4j_driver.session() as session:
            result = session.run(query, parameters=params or {})
            records = [record.data() for record in result]
            return {"results": records, "count": len(records)}
    except Exception as e:
        logger.error(f"Query execution error: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@app.post("/api/admin/create-indexes")
async def create_indexes():
    """
    Create indexes on Neo4j for query optimization.
    This endpoint should be called after initial setup or data import.
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")

    try:
        with neo4j_driver.session() as session:
            indexes_created = []
            
            # Create index on job_id for all nodes
            try:
                session.run("CREATE INDEX job_id_index IF NOT EXISTS FOR (n) ON (n.job_id)")
                indexes_created.append("job_id_index")
            except Exception as e:
                logger.warning(f"Index job_id_index may already exist: {e}")

            # Create index on repo_id for all nodes
            try:
                session.run("CREATE INDEX repo_id_index IF NOT EXISTS FOR (n) ON (n.repo_id)")
                indexes_created.append("repo_id_index")
            except Exception as e:
                logger.warning(f"Index repo_id_index may already exist: {e}")

            # Create indexes on common properties
            try:
                session.run("CREATE INDEX file_path_index IF NOT EXISTS FOR (f:File) ON (f.path)")
                indexes_created.append("file_path_index")
            except Exception as e:
                logger.warning(f"Index file_path_index may already exist: {e}")

            try:
                session.run("CREATE INDEX function_name_index IF NOT EXISTS FOR (fn:Function) ON (fn.name)")
                indexes_created.append("function_name_index")
            except Exception as e:
                logger.warning(f"Index function_name_index may already exist: {e}")

            try:
                session.run("CREATE INDEX class_name_index IF NOT EXISTS FOR (c:Class) ON (c.name)")
                indexes_created.append("class_name_index")
            except Exception as e:
                logger.warning(f"Index class_name_index may already exist: {e}")

            return {
                "message": "Indexes created successfully",
                "indexes": indexes_created,
                "count": len(indexes_created)
            }
    except Exception as e:
        logger.error(f"Index creation error: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/api/graph/{repo_id}/contributions")
async def get_file_contributions(repo_id: str):
    """
    Get git contribution metrics for all files in the repository.
    Returns commit counts, primary authors, and contributor information per file.
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")

    try:
        with neo4j_driver.session() as session:
            query = """
            MATCH (f:File)
            WHERE f.repo_id = $repo_id
            RETURN f.id as file_path,
                   f.commit_count as commit_count,
                   f.last_commit_date as last_commit_date,
                   f.primary_author as primary_author,
                   f.lines_changed_total as lines_changed_total,
                   f.contributors as contributors,
                   f.language as language
            ORDER BY f.commit_count DESC
            """
            result = session.run(query, repo_id=repo_id)
            
            contributions = []
            for record in result:
                contributions.append({
                    "file_path": record["file_path"],
                    "commit_count": record["commit_count"] or 0,
                    "last_commit_date": record["last_commit_date"],
                    "primary_author": record["primary_author"] or "",
                    "lines_changed_total": record["lines_changed_total"] or 0,
                    "contributors": record["contributors"] or [],
                    "contributor_count": len(record["contributors"] or []),
                    "language": record["language"]
                })
            
            logger.info(f"📊 Retrieved contributions for {len(contributions)} files in repo {repo_id}")
            return {
                "repo_id": repo_id,
                "total_files": len(contributions),
                "contributions": contributions
            }
    except Exception as e:
        logger.error(f"Error retrieving file contributions: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/api/graph/{repo_id}/boundaries")
async def get_module_boundaries(repo_id: str, boundary_type: Optional[str] = None):
    """
    Get detected module boundaries in the repository.
    
    Query Parameters:
        boundary_type: Filter by type (physical, logical, architectural)
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")

    try:
        with neo4j_driver.session() as session:
            # Build query based on filters
            where_clause = "b.repo_id = $repo_id"
            params = {"repo_id": repo_id}
            
            if boundary_type:
                where_clause += " AND b.type = $boundary_type"
                params["boundary_type"] = boundary_type
            
            query = f"""
            MATCH (b:Boundary)
            WHERE {where_clause}
            OPTIONAL MATCH (f:File)-[:BELONGS_TO]->(b)
            RETURN b.id as id,
                   b.name as name,
                   b.type as type,
                   b.path as path,
                   b.layer as layer,
                   b.file_count as file_count,
                   collect(f.id) as files
            ORDER BY b.type, b.name
            """
            result = session.run(query, **params)
            
            boundaries = []
            for record in result:
                boundaries.append({
                    "id": record["id"],
                    "name": record["name"],
                    "type": record["type"],
                    "path": record["path"],
                    "layer": record["layer"],
                    "file_count": record["file_count"],
                    "files": [f for f in record["files"] if f]  # Filter out nulls
                })
            
            logger.info(f"🗺️  Retrieved {len(boundaries)} boundaries in repo {repo_id}")
            return {
                "repo_id": repo_id,
                "total_boundaries": len(boundaries),
                "boundaries": boundaries
            }
    except Exception as e:
        logger.error(f"Error retrieving module boundaries: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/api/graph/{repo_id}/dependencies")
async def get_dependencies(
    repo_id: str,
    file_path: Optional[str] = None,
    dependency_type: Optional[str] = None
):
    """
    Get import/dependency relationships for a repository or specific file.
    
    Query Parameters:
    - file_path: Filter dependencies for a specific file
    - dependency_type: Filter by edge type (IMPORTS, CALLS, DEPENDS_ON)
    
    Returns dependency mappings with file-to-file and function-call relationships.
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")
    
    try:
        with neo4j_driver.session() as session:
            # Build query based on filters
            where_clauses = ["(f.repo_id = $repo_id OR f.job_id = $repo_id)"]
            params = {"repo_id": repo_id}
            
            if file_path:
                where_clauses.append("f.path = $file_path")
                params["file_path"] = file_path
            
            # Query for IMPORTS edges (file-to-module dependencies)
            edge_filter = ""
            if dependency_type and dependency_type.upper() == "IMPORTS":
                edge_filter = ":IMPORTS"
            elif dependency_type and dependency_type.upper() == "CALLS":
                edge_filter = ":CALLS"
            elif dependency_type:
                edge_filter = f":{dependency_type.upper()}"
            
            query = f"""
            MATCH (f:File)-[r{edge_filter}]->(target)
            WHERE {' AND '.join(where_clauses)}
            RETURN 
                f.path as source_file,
                f.language as source_language,
                type(r) as relationship_type,
                CASE 
                    WHEN target:File THEN target.path
                    WHEN target:Module THEN target.name
                    WHEN target:Function THEN target.name
                    WHEN target:Class THEN target.name
                    ELSE 'Unknown'
                END as target_name,
                labels(target)[0] as target_type
            ORDER BY f.path, relationship_type
            """
            
            result = session.run(query, **params)
            
            # Organize dependencies
            dependencies = []
            file_deps_map = {}
            
            for record in result:
                source = record["source_file"]
                target = record["target_name"]
                rel_type = record["relationship_type"]
                target_type = record["target_type"]
                
                dep_entry = {
                    "source_file": source,
                    "source_language": record["source_language"],
                    "target": target,
                    "target_type": target_type,
                    "relationship": rel_type
                }
                dependencies.append(dep_entry)
                
                # Build file-level dependency map (only for file-to-file or file-to-module)
                if target_type in ["File", "Module"]:
                    if source not in file_deps_map:
                        file_deps_map[source] = {
                            "language": record["source_language"],
                            "imports": [],
                            "imported_by": []
                        }
                    file_deps_map[source]["imports"].append({
                        "target": target,
                        "type": target_type
                    })
            
            # Query reverse dependencies if specific file requested
            reverse_deps = []
            if file_path:
                reverse_query = """
                MATCH (source)-[r]->(target)
                WHERE (target.path = $file_path OR target.name = $file_path)
                AND (source.repo_id = $repo_id OR source.job_id = $repo_id)
                RETURN 
                    source.path as dependent_file,
                    source.language as dependent_language,
                    type(r) as relationship_type,
                    labels(source)[0] as source_type
                """
                reverse_result = session.run(reverse_query, **params)
                
                for record in reverse_result:
                    if record["dependent_file"]:  # Only include if it's a file
                        reverse_deps.append({
                            "dependent_file": record["dependent_file"],
                            "dependent_language": record["dependent_language"],
                            "relationship": record["relationship_type"],
                            "source_type": record["source_type"]
                        })
            
            logger.info(f"📊 Retrieved {len(dependencies)} dependencies for repo {repo_id}" + 
                       (f" (file: {file_path})" if file_path else ""))
            
            response = {
                "repo_id": repo_id,
                "total_dependencies": len(dependencies),
                "dependencies": dependencies
            }
            
            if file_path:
                response["file_path"] = file_path
                response["reverse_dependencies"] = reverse_deps
                response["total_reverse_dependencies"] = len(reverse_deps)
            
            return response
            
    except Exception as e:
        logger.error(f"Error retrieving dependencies: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/api/graph/{repo_id}/dependency-tree/{file_path:path}")
async def get_dependency_tree(repo_id: str, file_path: str, max_depth: int = 3):
    """
    Get the full dependency tree for a specific file (recursive dependencies).
    
    Query Parameters:
    - max_depth: Maximum depth to traverse (default: 3)
    
    Returns a tree structure showing all transitive dependencies.
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")
    
    try:
        with neo4j_driver.session() as session:
            # Use Cypher path query to get dependency tree
            query = """
            MATCH path = (f:File {path: $file_path})-[:IMPORTS|CALLS*1..%d]->(target)
            WHERE (f.repo_id = $repo_id OR f.job_id = $repo_id)
            WITH path, target, length(path) as depth
            RETURN 
                [node in nodes(path) | 
                    CASE 
                        WHEN node:File THEN node.path
                        WHEN node:Module THEN node.name
                        WHEN node:Function THEN node.name
                        WHEN node:Class THEN node.name
                        ELSE 'Unknown'
                    END
                ] as dependency_chain,
                depth,
                labels(target)[0] as target_type
            ORDER BY depth, dependency_chain
            LIMIT 1000
            """ % max_depth
            
            result = session.run(query, repo_id=repo_id, file_path=file_path)
            
            paths = []
            unique_dependencies = set()
            
            for record in result:
                chain = record["dependency_chain"]
                depth = record["depth"]
                target_type = record["target_type"]
                
                paths.append({
                    "chain": chain,
                    "depth": depth,
                    "target_type": target_type
                })
                
                # Track unique direct and transitive dependencies
                for dep in chain[1:]:  # Skip the source file itself
                    unique_dependencies.add(dep)
            
            logger.info(f"🌳 Retrieved dependency tree for {file_path} in repo {repo_id}: "
                       f"{len(paths)} paths, {len(unique_dependencies)} unique dependencies")
            
            return {
                "repo_id": repo_id,
                "file_path": file_path,
                "max_depth": max_depth,
                "total_paths": len(paths),
                "unique_dependencies": len(unique_dependencies),
                "dependency_paths": paths,
                "all_dependencies": sorted(list(unique_dependencies))
            }
            
    except Exception as e:
        logger.error(f"Error retrieving dependency tree: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/api/graph/{repo_id}/dependency-graph")
async def get_full_dependency_graph(repo_id: str, limit: int = 500):
    """
    Get the complete dependency graph for visualization.
    Returns nodes and edges suitable for graph visualization tools.
    
    Query Parameters:
    - limit: Maximum number of edges to return (default: 500)
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")
    
    try:
        with neo4j_driver.session() as session:
            # Get all nodes
            nodes_query = """
            MATCH (n)
            WHERE (n.repo_id = $repo_id OR n.job_id = $repo_id)
            RETURN 
                CASE 
                    WHEN n:File THEN n.path
                    WHEN n:Module THEN n.name
                    WHEN n:Function THEN n.name
                    WHEN n:Class THEN n.name
                    ELSE id(n)
                END as id,
                CASE 
                    WHEN n:File THEN n.path
                    WHEN n:Module THEN n.name
                    WHEN n:Function THEN n.name
                    WHEN n:Class THEN n.name
                    ELSE 'Unknown'
                END as label,
                labels(n)[0] as type,
                properties(n) as properties
            LIMIT $limit
            """
            
            # Get all edges
            edges_query = """
            MATCH (source)-[r:IMPORTS|CALLS|DEFINES|CONTAINS|INHERITS|BELONGS_TO]->(target)
            WHERE (source.repo_id = $repo_id OR source.job_id = $repo_id)
            RETURN 
                CASE 
                    WHEN source:File THEN source.path
                    WHEN source:Module THEN source.name
                    WHEN source:Function THEN source.name
                    WHEN source:Class THEN source.name
                    ELSE id(source)
                END as source,
                CASE 
                    WHEN target:File THEN target.path
                    WHEN target:Module THEN target.name
                    WHEN target:Function THEN target.name
                    WHEN target:Class THEN target.name
                    WHEN target:Boundary THEN target.name
                    ELSE id(target)
                END as target,
                type(r) as type
            LIMIT $limit
            """
            
            nodes_result = session.run(nodes_query, repo_id=repo_id, limit=limit)
            edges_result = session.run(edges_query, repo_id=repo_id, limit=limit)
            
            nodes = []
            edges = []
            
            for record in nodes_result:
                nodes.append({
                    "id": record["id"],
                    "label": record["label"],
                    "type": record["type"],
                    "properties": record["properties"]
                })
            
            for record in edges_result:
                edges.append({
                    "source": record["source"],
                    "target": record["target"],
                    "type": record["type"]
                })
            
            logger.info(f"🔗 Retrieved dependency graph for repo {repo_id}: "
                       f"{len(nodes)} nodes, {len(edges)} edges")
            
            return {
                "repo_id": repo_id,
                "total_nodes": len(nodes),
                "total_edges": len(edges),
                "nodes": nodes,
                "edges": edges
            }
            
    except Exception as e:
        logger.error(f"Error retrieving dependency graph: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/api/pagerank/{repo_id}")
async def calculate_pagerank(repo_id: str):
    """
    Calculate PageRank for nodes in the repository graph.
    Identifies the most important/central functions and classes.
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")

    try:
        # Build NetworkX graph from Neo4j
        with neo4j_driver.session() as session:
            query = """
            MATCH (a)-[r:CALLS|IMPORTS]->(b)
            WHERE (a.repo_id = $repo_id OR a.job_id = $repo_id) AND (b.repo_id = $repo_id OR b.job_id = $repo_id)
            RETURN a.id as source, b.id as target
            """
            result = session.run(query, repo_id=repo_id)
            
            G = nx.DiGraph()
            for record in result:
                G.add_edge(record["source"], record["target"])

        # Calculate PageRank
        if len(G.nodes()) > 0:
            pagerank = nx.pagerank(G)
            # Sort by PageRank score
            sorted_nodes = sorted(pagerank.items(), key=lambda x: x[1], reverse=True)
            
            return {
                "repo_id": repo_id,
                "total_nodes": len(G.nodes()),
                "top_nodes": [
                    {"id": node_id, "score": round(score, 6)}
                    for node_id, score in sorted_nodes[:20]
                ]
            }
        else:
            return {
                "repo_id": repo_id,
                "total_nodes": 0,
                "top_nodes": []
            }
    except Exception as e:
        logger.error(f"PageRank calculation error: {e}")
        raise HTTPException(status_code=500, detail=str(e))


# Shutdown event
@app.on_event("shutdown")
async def shutdown_event():
    logger.info("🛑 Shutting down Graph Engine...")
    if neo4j_driver:
        try:
            neo4j_driver.close()
            logger.info("✅ Neo4j driver closed successfully")
        except Exception as e:
            logger.error(f"❌ Error closing Neo4j driver: {e}")
    logger.info("👋 Graph Engine shutdown complete")


if __name__ == "__main__":
    import uvicorn
    
    # Configure uvicorn with graceful shutdown timeout
    uvicorn.run(
        app,
        host="0.0.0.0",
        port=8000,
        timeout_graceful_shutdown=30  # 30-second shutdown timeout
    )
