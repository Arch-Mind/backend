# Graph Engine (Python)

The Graph Engine is responsible for analyzing the dependency graph stored in Neo4j. It provides APIs for impact analysis, metrics calculation, PageRank computation, and graph visualization.

## Technology Stack

- **Language**: Python 3.11+
- **Framework**: FastAPI
- **Graph Database**: Neo4j (via neo4j-python-driver)
- **Graph Algorithms**: NetworkX
- **Server**: Uvicorn (ASGI)

## Features

- ✅ FastAPI REST API
- ✅ Neo4j graph database connection
- ✅ Impact analysis (affected nodes)
- ✅ Repository metrics calculation
- ✅ Dependency graph retrieval
- ✅ PageRank calculation with NetworkX
- ✅ Custom Cypher query execution
- ✅ CORS support for web dashboard
- ✅ Health check endpoint

## Architecture

```
┌────────────────┐
│   Client/UI    │
└────────┬───────┘
         │
         ▼
┌────────────────────┐
│  FastAPI Server    │
│  (main.py)         │
└──────┬─────────────┘
       │
       ├─► Impact Analysis
       │   └─► Neo4j Cypher Query
       │
       ├─► Metrics Calculation
       │   └─► Aggregate Counts
       │
       ├─► PageRank Computation
       │   ├─► Fetch graph from Neo4j
       │   └─► NetworkX PageRank
       │
       └─► Graph Visualization
           └─► Nodes & Edges
```

## Getting Started

### Prerequisites

- Python 3.11 or higher
- Running Neo4j instance
- pip or poetry

### Installation

```bash
# Create virtual environment
python -m venv venv

# Activate virtual environment
# On Windows:
venv\Scripts\activate
# On Linux/Mac:
source venv/bin/activate

# Install dependencies
pip install -r requirements.txt

# Copy environment configuration
cp .env.example .env

# Edit .env with your configuration
```

### Running Locally

```bash
# Development mode with auto-reload
uvicorn main:app --reload

# Production mode
uvicorn main:app --host 0.0.0.0 --port 8000

# With custom port
uvicorn main:app --port 8001
```

The server will start on `http://localhost:8000`

## API Endpoints

### Health Check
```bash
GET /health
```

**Response:**
```json
{
  "status": "ok",
  "services": {
    "neo4j": "healthy"
  }
}
```

### Impact Analysis
```bash
GET /api/impact/{node_id}?depth=3
```

Analyzes the impact of changes to a specific node.

**Response:**
```json
{
  "node_id": "function-main",
  "impacted_count": 15,
  "impacted_nodes": [
    {
      "id": "function-process",
      "name": "process",
      "type": "Function",
      "distance": 1
    }
  ]
}
```

### Repository Metrics
```bash
GET /api/metrics/{repo_id}
```

**Response:**
```json
{
  "total_files": 42,
  "total_functions": 156,
  "total_classes": 23,
  "total_dependencies": 384,
  "complexity_score": 24.62
}
```

### Dependency Graph
```bash
GET /api/graph/{repo_id}?limit=100
```

**Response:**
```json
{
  "nodes": [
    {
      "id": "file-main.rs",
      "label": "main.rs",
      "type": "File",
      "properties": {}
    }
  ],
  "edges": [
    {
      "source": "function-main",
      "target": "function-process",
      "type": "CALLS"
    }
  ]
}
```

### PageRank Calculation
```bash
GET /api/pagerank/{repo_id}
```

Calculates PageRank to identify most important nodes.

**Response:**
```json
{
  "repo_id": "repo-123",
  "total_nodes": 156,
  "top_nodes": [
    {
      "id": "function-main",
      "score": 0.045231
    }
  ]
}
```

### Custom Cypher Query
```bash
POST /api/query
Content-Type: application/json

{
  "query": "MATCH (n:Function) RETURN n.name LIMIT 10",
  "params": {}
}
```

## Graph Algorithms

### Impact Analysis
Uses Neo4j's graph traversal to find all nodes connected within a specified depth:

```cypher
MATCH path = (n {id: $node_id})-[:CALLS|IMPORTS|INHERITS*1..3]-(impacted)
RETURN DISTINCT impacted
```

### PageRank
Uses NetworkX to calculate PageRank scores:
1. Fetch graph structure from Neo4j
2. Build NetworkX DiGraph
3. Compute PageRank
4. Return top-ranked nodes

### Complexity Score
Simple heuristic: `(total_dependencies / total_functions) * 10`

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `NEO4J_URI` | Neo4j Bolt URI | `bolt://localhost:7687` |
| `NEO4J_USER` | Neo4j username | `neo4j` |
| `NEO4J_PASSWORD` | Neo4j password | `password` |
| `HOST` | Server host | `0.0.0.0` |
| `PORT` | Server port | `8000` |
| `LOG_LEVEL` | Logging level | `INFO` |

## Testing

```bash
# Run tests
pytest

# Run with coverage
pytest --cov=.

# Test the API
curl http://localhost:8000/health

# Get repository metrics
curl http://localhost:8000/api/metrics/repo-123
```

## API Documentation

FastAPI automatically generates interactive API documentation:

- **Swagger UI**: http://localhost:8000/docs
- **ReDoc**: http://localhost:8000/redoc
- **OpenAPI JSON**: http://localhost:8000/openapi.json

## Deployment

```bash
# Using Gunicorn (production)
gunicorn main:app --workers 4 --worker-class uvicorn.workers.UvicornWorker --bind 0.0.0.0:8000

# Using Docker
docker build -t archmind/graph-engine:latest .
docker run -p 8000:8000 archmind/graph-engine:latest
```

## Future Enhancements

- [ ] Cycle detection algorithm
- [ ] Community detection (Louvain)
- [ ] Betweenness centrality calculation
- [ ] Shortest path analysis
- [ ] Graph clustering
- [ ] Anomaly detection
- [ ] Code smell identification
- [ ] Architecture pattern recognition
- [ ] Real-time graph updates via WebSockets
- [ ] Caching layer with Redis
- [ ] Query optimization
- [ ] Graph export (GraphML, GEXF)
