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
    """Check if repo_id exists in the database."""
    try:
        result = session.run(
            "MATCH (n {repo_id: $repo_id}) RETURN count(n) as count LIMIT 1",
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

            # Count files - using job_id property
            files_result = session.run(
                "MATCH (f:File {repo_id: $repo_id}) RETURN count(f) as count",
                repo_id=repo_id
            )
            files_record = files_result.single()
            total_files = files_record["count"] if files_record else 0

            # Count functions
            functions_result = session.run(
                "MATCH (fn:Function {repo_id: $repo_id}) RETURN count(fn) as count",
                repo_id=repo_id
            )
            functions_record = functions_result.single()
            total_functions = functions_record["count"] if functions_record else 0

            # Count classes
            classes_result = session.run(
                "MATCH (c:Class {repo_id: $repo_id}) RETURN count(c) as count",
                repo_id=repo_id
            )
            classes_record = classes_result.single()
            total_classes = classes_record["count"] if classes_record else 0

            # Count dependencies (edges don't have job_id, count by matching nodes)
            deps_result = session.run(
                "MATCH (a {repo_id: $repo_id})-[r:CALLS|IMPORTS|INHERITS]->(b {repo_id: $repo_id}) RETURN count(r) as count",
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
            total_nodes_query = "MATCH (n {repo_id: $repo_id}) RETURN count(n) as count"
            total_nodes = await get_total_count(session, total_nodes_query, repo_id)

            # Get total count of edges
            total_edges_query = "MATCH (a {repo_id: $repo_id})-[r]->(b {repo_id: $repo_id}) RETURN count(r) as count"
            total_edges = await get_total_count(session, total_edges_query, repo_id)

            # Get nodes with pagination
            nodes_query = """
            MATCH (n {repo_id: $repo_id})
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
            MATCH (a {repo_id: $repo_id})-[r]->(b {repo_id: $repo_id})
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
            MATCH (a {repo_id: $repo_id})-[r:CALLS|IMPORTS]->(b {repo_id: $repo_id})
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
