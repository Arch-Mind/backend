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

try:
    neo4j_driver = GraphDatabase.driver(neo4j_uri, auth=(neo4j_user, neo4j_password))
    logger.info(f"✅ Connected to Neo4j at {neo4j_uri}")
except Exception as e:
    logger.error(f"Failed to connect to Neo4j: {e}")
    neo4j_driver = None


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
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")

    try:
        with neo4j_driver.session() as session:
            # Count files
            files_result = session.run(
                "MATCH (f:File {repo_id: $repo_id}) RETURN count(f) as count",
                repo_id=repo_id
            )
            total_files = files_result.single()["count"]

            # Count functions
            functions_result = session.run(
                "MATCH (fn:Function {repo_id: $repo_id}) RETURN count(fn) as count",
                repo_id=repo_id
            )
            total_functions = functions_result.single()["count"]

            # Count classes
            classes_result = session.run(
                "MATCH (c:Class {repo_id: $repo_id}) RETURN count(c) as count",
                repo_id=repo_id
            )
            total_classes = classes_result.single()["count"]

            # Count dependencies
            deps_result = session.run(
                "MATCH ()-[r:CALLS|IMPORTS|INHERITS]->() WHERE r.repo_id = $repo_id RETURN count(r) as count",
                repo_id=repo_id
            )
            total_dependencies = deps_result.single()["count"]

            # Calculate complexity score (simplified)
            complexity_score = (total_dependencies / max(total_functions, 1)) * 10

            return MetricsResponse(
                total_files=total_files,
                total_functions=total_functions,
                total_classes=total_classes,
                total_dependencies=total_dependencies,
                complexity_score=round(complexity_score, 2)
            )
    except Exception as e:
        logger.error(f"Metrics calculation error: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@app.get("/api/graph/{repo_id}")
async def get_dependency_graph(repo_id: str, limit: int = 100):
    """
    Retrieve the full dependency graph for a repository.
    """
    if not neo4j_driver:
        raise HTTPException(status_code=503, detail="Neo4j connection not available")

    try:
        with neo4j_driver.session() as session:
            # Get nodes
            nodes_query = """
            MATCH (n {repo_id: $repo_id})
            RETURN n.id as id, n.name as name, labels(n)[0] as type, properties(n) as props
            LIMIT $limit
            """
            nodes_result = session.run(nodes_query, repo_id=repo_id, limit=limit)
            nodes = [
                GraphNode(
                    id=record["id"],
                    label=record["name"],
                    type=record["type"],
                    properties=record["props"]
                )
                for record in nodes_result
            ]

            # Get edges
            edges_query = """
            MATCH (a {repo_id: $repo_id})-[r]->(b {repo_id: $repo_id})
            RETURN a.id as source, b.id as target, type(r) as type
            LIMIT $limit
            """
            edges_result = session.run(edges_query, repo_id=repo_id, limit=limit)
            edges = [
                GraphEdge(
                    source=record["source"],
                    target=record["target"],
                    type=record["type"]
                )
                for record in edges_result
            ]

            return GraphResponse(nodes=nodes, edges=edges)
    except Exception as e:
        logger.error(f"Graph retrieval error: {e}")
        raise HTTPException(status_code=500, detail=str(e))


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
    if neo4j_driver:
        neo4j_driver.close()
        logger.info("Neo4j connection closed")


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
