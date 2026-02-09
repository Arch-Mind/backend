# ArchMind API Documentation

## Overview

ArchMind consists of two main API services:

### API Gateway (Go/Gin)
The API Gateway is a RESTful API built with Go (Gin framework) that provides endpoints for:
- Repository analysis job management
- Job status tracking
- Repository management and querying
- **GitHub Webhooks for automatic analysis on code changes**

The API uses PostgreSQL for persistent storage and Redis for job queue management.

### Graph Engine (Python/FastAPI)
The Graph Engine provides graph intelligence and analysis capabilities:
- Impact analysis for code changes
- Repository metrics and statistics
- Dependency graph visualization
- PageRank analysis for code importance
- Custom Cypher query execution

## Base URLs

| Service | URL |
|---------|-----|
| API Gateway | `http://localhost:8080` |
| Graph Engine | `http://localhost:8000` |

## Environment Configuration

### Required Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8080` | API Gateway port |
| `POSTGRES_URL` | `postgresql://postgres:postgres@localhost:5432/archmind?sslmode=disable` | PostgreSQL connection string |
| `REDIS_URL` | `localhost:6379` | Redis connection address |
| `REDIS_PASSWORD` | `` | Redis password (if required) |
| `GITHUB_WEBHOOK_SECRET` | `` | Secret for verifying GitHub webhook signatures |

### Graph Engine Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `NEO4J_URI` | `bolt://localhost:7687` | Neo4j connection URI |
| `NEO4J_USER` | `neo4j` | Neo4j username |
| `NEO4J_PASSWORD` | `password` | Neo4j password |

### Example .env file

```env
PORT=8080
POSTGRES_URL=postgresql://postgres:postgres@localhost:5432/archmind?sslmode=disable
REDIS_URL=localhost:6379
REDIS_PASSWORD=
GITHUB_WEBHOOK_SECRET=your_webhook_secret_here

# Graph Engine
NEO4J_URI=bolt://localhost:7687
NEO4J_USER=neo4j
NEO4J_PASSWORD=password
```

## API Gateway Endpoints

### 1. Health Check

**Endpoint:** `GET /health`

**Description:** Returns the health status of the API Gateway and its dependent services.

**Response:**
```json
{
  "status": "ok",
  "services": {
    "redis": "healthy",
    "postgres": "healthy"
  },
  "timestamp": "2026-01-27T10:30:00Z"
}
```

**Status Codes:**
- `200 OK` - Service is healthy

---

### 2. Analyze Repository

**Endpoint:** `POST /api/v1/analyze`

**Description:** Creates a new analysis job for a repository. The job is queued in Redis for processing by the ingestion worker.

**Request Headers:**
```
Content-Type: application/json
```

**Request Body:**
```json
{
  "repo_url": "https://github.com/user/repository.git",
  "branch": "main",
  "options": {
    "key1": "value1",
    "key2": "value2"
  }
}
```

**Request Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `repo_url` | string | Yes | Git repository URL |
| `branch` | string | No | Git branch to analyze (defaults to `main`) |
| `options` | object | No | Additional configuration options for analysis |

**Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "QUEUED",
  "message": "Analysis job created successfully",
  "created_at": "2026-01-27T10:30:00Z"
}
```

**Status Codes:**
- `201 Created` - Job created successfully
- `400 Bad Request` - Invalid request body (missing `repo_url`)
- `500 Internal Server Error` - Failed to create job (database or Redis error)

**Example cURL:**
```bash
curl -X POST http://localhost:8080/api/v1/analyze \
  -H "Content-Type: application/json" \
  -d '{
    "repo_url": "https://github.com/facebook/react.git",
    "branch": "main",
    "options": {
      "depth": "2",
      "include_tests": "false"
    }
  }'
```

---

### 3. Get Job Status

**Endpoint:** `GET /api/v1/jobs/:id`

**Description:** Retrieves the status and details of a specific analysis job.

**Path Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `id` | string | Job ID (UUID) |

**Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "repo_url": "https://github.com/facebook/react.git",
  "branch": "main",
  "status": "QUEUED",
  "options": {
    "depth": "2",
    "include_tests": "false"
  },
  "created_at": "2026-01-27T10:30:00Z"
}
```

**Possible Status Values:**
- `QUEUED` - Job is waiting in the queue
- `PROCESSING` - Job is currently being processed
- `COMPLETED` - Job completed successfully
- `FAILED` - Job failed during processing

**Status Codes:**
- `200 OK` - Job found
- `404 Not Found` - Job not found
- `500 Internal Server Error` - Database error

**Example cURL:**
```bash
curl http://localhost:8080/api/v1/jobs/550e8400-e29b-41d4-a716-446655440000
```

---

## GitHub Webhooks

The API Gateway supports GitHub webhooks for automatic code analysis when changes are pushed to a repository.

### Webhook Endpoint

**Endpoint:** `POST /webhooks/github`

**Description:** Receives webhook events from GitHub and automatically triggers code analysis for relevant events.

### Supported Events

| Event | Actions Processed | Description |
|-------|------------------|-------------|
| `push` | All pushes | Triggered when code is pushed to any branch |
| `pull_request` | `opened`, `synchronize`, `reopened` | Triggered for PR activities |
| `ping` | N/A | Sent when webhook is first configured |

### Security: Signature Verification

All webhook requests are verified using HMAC-SHA256 signatures. Configure the same secret in both GitHub and your environment:

1. Generate a secure secret:
   ```bash
   openssl rand -hex 32
   ```

2. Set the secret in your `.env` file:
   ```env
   GITHUB_WEBHOOK_SECRET=your_generated_secret
   ```

3. Configure the same secret in GitHub:
   - Go to your repository → Settings → Webhooks → Add webhook
   - Set Payload URL: `https://your-api.com/webhooks/github`
   - Set Content type: `application/json`
   - Set Secret: `your_generated_secret`
   - Select events: `Push events` and `Pull requests`

### Request Headers (from GitHub)

| Header | Description |
|--------|-------------|
| `X-GitHub-Event` | Event type (`push`, `pull_request`, `ping`) |
| `X-Hub-Signature-256` | HMAC-SHA256 signature for verification |
| `X-GitHub-Delivery` | Unique delivery ID for this event |

### Response Format

```json
{
  "status": "queued",
  "message": "Analysis job created",
  "job_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Possible Status Values:**
- `queued` - Analysis job was created and queued
- `skipped` - Event was valid but no analyzable files changed
- `ignored` - Event type or action is not processed
- `ok` - Successful acknowledgment (for ping events)
- `error` - An error occurred processing the webhook

### File Type Filtering

The webhook handler only triggers analysis when relevant code files are changed:

| Extensions | Language |
|------------|----------|
| `.ts`, `.tsx` | TypeScript |
| `.js`, `.jsx` | JavaScript |
| `.go` | Go |
| `.rs` | Rust |
| `.py` | Python |
| `.java` | Java |

If a push only modifies non-code files (e.g., README.md, images), the webhook returns `skipped` without creating a job.

### Example: Push Event

**GitHub sends:**
```json
{
  "ref": "refs/heads/main",
  "repository": {
    "clone_url": "https://github.com/user/repo.git",
    "full_name": "user/repo"
  },
  "commits": [
    {
      "id": "abc123",
      "message": "Add new feature",
      "added": ["src/feature.ts"],
      "modified": ["src/index.ts"],
      "removed": []
    }
  ]
}
```

**ArchMind responds:**
```json
{
  "status": "queued",
  "message": "Analysis job created",
  "job_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

### Example: Pull Request Event

**GitHub sends:**
```json
{
  "action": "opened",
  "number": 42,
  "pull_request": {
    "head": {
      "ref": "feature-branch",
      "sha": "abc123"
    }
  },
  "repository": {
    "clone_url": "https://github.com/user/repo.git"
  }
}
```

**ArchMind responds:**
```json
{
  "status": "queued",
  "message": "Analysis job created for PR #42",
  "job_id": "550e8400-e29b-41d4-a716-446655440001"
}
```

### Testing Webhooks Locally

Use [ngrok](https://ngrok.com/) to expose your local server:

```bash
# Start your API Gateway
go run main.go

# In another terminal, expose it via ngrok
ngrok http 8080
```

Configure the ngrok URL (`https://xxxx.ngrok.io/webhooks/github`) in your GitHub webhook settings.

### Webhook Job Options

Jobs created via webhooks include additional metadata in the `options` field:

| Option | Description |
|--------|-------------|
| `trigger` | Event type (`push` or `pull_request`) |
| `source` | Always `webhook` for webhook-triggered jobs |
| `changed_files` | JSON array of changed file paths (push events only) |
| `files_truncated` | Set to `true` if changed files list was truncated |

---

### 4. List Jobs

**Endpoint:** `GET /api/v1/jobs`

**Description:** Retrieves a list of analysis jobs (up to 50 most recent).

**Query Parameters:** None

**Response:**
```json
{
  "jobs": [
    {
      "job_id": "550e8400-e29b-41d4-a716-446655440000",
      "repo_url": "https://github.com/facebook/react.git",
      "branch": "main",
      "status": "QUEUED",
      "options": {
        "depth": "2"
      },
      "created_at": "2026-01-27T10:30:00Z"
    },
    {
      "job_id": "550e8400-e29b-41d4-a716-446655440001",
      "repo_url": "https://github.com/vuejs/vue.git",
      "branch": "main",
      "status": "PROCESSING",
      "options": null,
      "created_at": "2026-01-27T10:25:00Z"
    }
  ],
  "total": 2
}
```

**Status Codes:**
- `200 OK` - Success
- `500 Internal Server Error` - Database error

**Example cURL:**
```bash
curl http://localhost:8080/api/v1/jobs
```

---

### 5. List Repositories

**Endpoint:** `GET /api/v1/repositories`

**Description:** Retrieves all tracked repositories in the system.

**Query Parameters:** None

**Response:**
```json
{
  "repositories": [
    {
      "id": 1,
      "url": "https://github.com/facebook/react.git",
      "owner_id": 1,
      "created_at": "2026-01-27T10:30:00Z"
    },
    {
      "id": 2,
      "url": "https://github.com/vuejs/vue.git",
      "owner_id": 2,
      "created_at": "2026-01-27T10:25:00Z"
    }
  ],
  "total": 2
}
```

**Status Codes:**
- `200 OK` - Success
- `500 Internal Server Error` - Database error

**Example cURL:**
```bash
curl http://localhost:8080/api/v1/repositories
```

---

### 6. Get Repository

**Endpoint:** `GET /api/v1/repositories/:id`

**Description:** Retrieves details of a specific repository.

**Path Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `id` | integer | Repository ID |

**Response:**
```json
{
  "id": 1,
  "url": "https://github.com/facebook/react.git",
  "owner_id": 1,
  "created_at": "2026-01-27T10:30:00Z"
}
```

**Status Codes:**
- `200 OK` - Repository found
- `404 Not Found` - Repository not found
- `500 Internal Server Error` - Database error

**Example cURL:**
```bash
curl http://localhost:8080/api/v1/repositories/1
```

---

## Graph Engine Endpoints

The Graph Engine runs on port 8000 and provides graph-based analysis features.

### 1. Graph Engine Root

**Endpoint:** `GET /`

**Description:** Returns basic service information.

**Response:**
```json
{
  "service": "ArchMind Graph Engine",
  "version": "0.1.0",
  "status": "running"
}
```

**Status Codes:**
- `200 OK` - Service is running

---

### 2. Graph Engine Health Check

**Endpoint:** `GET /health`

**Description:** Returns the health status of the Graph Engine and Neo4j connection.

**Response:**
```json
{
  "status": "ok",
  "services": {
    "neo4j": "healthy"
  }
}
```

**Possible Neo4j Status Values:**
- `healthy` - Connected and responding
- `unhealthy` - Connection failed
- `disconnected` - Not configured

**Status Codes:**
- `200 OK` - Service is running

**Example cURL:**
```bash
curl http://localhost:8000/health
```

---

### 3. Impact Analysis

**Endpoint:** `GET /api/impact/{node_id}`

**Description:** Analyze the impact of changes to a specific node. Returns all nodes that would be affected if this node changes by following `CALLS`, `IMPORTS`, and `INHERITS` relationships.

**Path Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `node_id` | string | The unique identifier of the node to analyze |

**Query Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `depth` | integer | `3` | Maximum relationship depth to traverse (1-3) |

**Response:**
```json
{
  "node_id": "src/utils/helper.ts::formatDate",
  "impacted_count": 5,
  "impacted_nodes": [
    {
      "id": "src/components/DatePicker.tsx::render",
      "name": "render",
      "type": "Function",
      "distance": 1
    },
    {
      "id": "src/views/Dashboard.tsx::Dashboard",
      "name": "Dashboard",
      "type": "Class",
      "distance": 2
    }
  ]
}
```

**Status Codes:**
- `200 OK` - Analysis completed
- `503 Service Unavailable` - Neo4j connection not available
- `500 Internal Server Error` - Analysis error

**Example cURL:**
```bash
curl "http://localhost:8000/api/impact/src%2Futils%2Fhelper.ts%3A%3AformatDate?depth=2"
```

---

### 4. Repository Metrics

**Endpoint:** `GET /api/metrics/{repo_id}`

**Description:** Calculate and return metrics for a repository. The `repo_id` is the `job_id` from the analysis job.

**Path Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `repo_id` | string | The job ID from the analysis job (UUID) |

**Response:**
```json
{
  "total_files": 45,
  "total_functions": 230,
  "total_classes": 28,
  "total_dependencies": 512,
  "complexity_score": 22.26
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `total_files` | integer | Number of files in the repository |
| `total_functions` | integer | Number of functions detected |
| `total_classes` | integer | Number of classes detected |
| `total_dependencies` | integer | Number of relationships (CALLS, IMPORTS, INHERITS) |
| `complexity_score` | float | Calculated complexity score (dependencies/functions * 10) |

**Status Codes:**
- `200 OK` - Metrics calculated
- `503 Service Unavailable` - Neo4j connection not available
- `500 Internal Server Error` - Calculation error

**Example cURL:**
```bash
curl http://localhost:8000/api/metrics/550e8400-e29b-41d4-a716-446655440000
```

---

### 5. Dependency Graph

**Endpoint:** `GET /api/graph/{repo_id}`

**Description:** Retrieve the full dependency graph for a repository for visualization.

**Path Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `repo_id` | string | The job ID from the analysis job (UUID) |

**Query Parameters:**

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | integer | `100` | Maximum number of nodes and edges to return |

**Response:**
```json
{
  "nodes": [
    {
      "id": "src/index.ts",
      "label": "index.ts",
      "type": "File",
      "properties": {
        "path": "src/index.ts",
        "job_id": "550e8400-e29b-41d4-a716-446655440000"
      }
    },
    {
      "id": "src/utils/helper.ts::formatDate",
      "label": "formatDate",
      "type": "Function",
      "properties": {
        "name": "formatDate",
        "job_id": "550e8400-e29b-41d4-a716-446655440000"
      }
    }
  ],
  "edges": [
    {
      "source": "src/index.ts",
      "target": "src/utils/helper.ts::formatDate",
      "type": "CALLS"
    }
  ]
}
```

**Node Types:**
- `File` - Source code file
- `Function` - Function or method
- `Class` - Class definition

**Edge Types:**
- `CALLS` - Function/method invocation
- `IMPORTS` - Module/file import
- `INHERITS` - Class inheritance

**Status Codes:**
- `200 OK` - Graph retrieved
- `503 Service Unavailable` - Neo4j connection not available
- `500 Internal Server Error` - Retrieval error

**Example cURL:**
```bash
curl "http://localhost:8000/api/graph/550e8400-e29b-41d4-a716-446655440000?limit=50"
```

---

### 6. Custom Cypher Query

**Endpoint:** `POST /api/query`

**Description:** Execute a custom Cypher query against the Neo4j database. **Use with caution** - this endpoint allows arbitrary read/write operations.

**Query Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | Yes | Cypher query to execute |
| `params` | object | No | Query parameters as JSON (recommended for security) |

**Response:**
```json
{
  "results": [
    {"name": "formatDate"},
    {"name": "parseJSON"},
    {"name": "validateInput"}
  ],
  "count": 3
}
```

**Status Codes:**
- `200 OK` - Query executed
- `503 Service Unavailable` - Neo4j connection not available
- `500 Internal Server Error` - Query execution error

**Example cURL:**
```bash
curl -X POST "http://localhost:8000/api/query?query=MATCH%20(f:File)%20RETURN%20f.path%20as%20path%20LIMIT%205"
```

With parameters (URL-encoded JSON):
```bash
curl -X POST "http://localhost:8000/api/query?query=MATCH%20(n:Function%20{job_id:%20\$job_id})%20RETURN%20n.name%20LIMIT%2010&params={\"job_id\":\"550e8400-e29b-41d4-a716-446655440000\"}"
```

---

### 7. PageRank Analysis

**Endpoint:** `GET /api/pagerank/{repo_id}`

**Description:** Calculate PageRank for nodes in the repository graph. This identifies the most important/central functions and classes based on how frequently they are called or imported.

**Path Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `repo_id` | string | The job ID from the analysis job (UUID) |

**Response:**
```json
{
  "repo_id": "550e8400-e29b-41d4-a716-446655440000",
  "total_nodes": 156,
  "top_nodes": [
    {
      "id": "src/utils/logger.ts::log",
      "score": 0.089234
    },
    {
      "id": "src/db/connection.ts::query",
      "score": 0.067891
    },
    {
      "id": "src/auth/validate.ts::validateToken",
      "score": 0.045678
    }
  ]
}
```

**Response Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `repo_id` | string | The repository/job ID |
| `total_nodes` | integer | Total number of nodes in the graph |
| `top_nodes` | array | Top 20 nodes by PageRank score |
| `top_nodes[].id` | string | Node identifier |
| `top_nodes[].score` | float | PageRank score (higher = more central/important) |

**Status Codes:**
- `200 OK` - PageRank calculated
- `503 Service Unavailable` - Neo4j connection not available
- `500 Internal Server Error` - Calculation error

**Example cURL:**
```bash
curl http://localhost:8000/api/pagerank/550e8400-e29b-41d4-a716-446655440000
```

---

## CORS Configuration

### API Gateway

The API Gateway has CORS (Cross-Origin Resource Sharing) enabled with the following settings:

| Setting | Value |
|---------|-------|
| Allowed Origins | `http://localhost:3000` |
| Allowed Methods | GET, POST, PUT, DELETE, OPTIONS |
| Allowed Headers | Origin, Content-Type, Authorization |
| Exposed Headers | Content-Length |
| Credentials | Allowed |
| Max Age | 12 hours |

To allow additional origins, modify the CORS configuration in `main.go`.

### Graph Engine

The Graph Engine also has CORS enabled:

| Setting | Value |
|---------|-------|
| Allowed Origins | `http://localhost:3000` |
| Allowed Methods | All (`*`) |
| Allowed Headers | All (`*`) |
| Credentials | Allowed |

To allow additional origins, modify the CORS configuration in `main.py`.

---

## Error Handling

All API endpoints return standardized error responses:

```json
{
  "error": "Error message",
  "details": "Additional error details (if available)"
}
```

### Common Error Codes

| Code | Meaning |
|------|---------|
| `400` | Bad Request - Invalid input or missing required fields |
| `404` | Not Found - Resource doesn't exist |
| `500` | Internal Server Error - Server-side error |

---

## Testing the API

### Prerequisites

1. **PostgreSQL** running with the ArchMind schema
2. **Redis** running on the specified address
3. **Neo4j** running for graph analysis
4. **API Gateway** running on port 8080
5. **Graph Engine** running on port 8000

### API Gateway Test Sequence

#### 1. Health Check
```bash
curl http://localhost:8080/health
```

Expected response:
```json
{
  "status": "ok",
  "services": {
    "redis": "healthy",
    "postgres": "healthy"
  },
  "timestamp": "2026-01-27T10:30:00Z"
}
```

#### 2. Create Analysis Job
```bash
JOB_ID=$(curl -s -X POST http://localhost:8080/api/v1/analyze \
  -H "Content-Type: application/json" \
  -d '{
    "repo_url": "https://github.com/facebook/react.git",
    "branch": "main"
  }' | jq -r '.job_id')

echo "Created job: $JOB_ID"
```

#### 3. Check Job Status
```bash
curl http://localhost:8080/api/v1/jobs/$JOB_ID
```

#### 4. List All Jobs
```bash
curl http://localhost:8080/api/v1/jobs
```

#### 5. List Repositories
```bash
curl http://localhost:8080/api/v1/repositories
```

#### 6. Get Specific Repository
```bash
curl http://localhost:8080/api/v1/repositories/1
```

### Graph Engine Test Sequence

#### 1. Health Check
```bash
curl http://localhost:8000/health
```

Expected response:
```json
{
  "status": "ok",
  "services": {
    "neo4j": "healthy"
  }
}
```

#### 2. Get Repository Metrics
```bash
curl http://localhost:8000/api/metrics/$JOB_ID
```

#### 3. Get Dependency Graph
```bash
curl "http://localhost:8000/api/graph/$JOB_ID?limit=50"
```

#### 4. Get Impact Analysis
```bash
curl "http://localhost:8000/api/impact/your_node_id?depth=2"
```

#### 5. Calculate PageRank
```bash
curl http://localhost:8000/api/pagerank/$JOB_ID
```

#### 6. Execute Custom Query
```bash
curl -X POST http://localhost:8000/api/query \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n) RETURN count(n) as total"}'
```

---

## Data Models

### AnalysisJob

Represents a code analysis job in the system.

```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "repo_url": "https://github.com/facebook/react.git",
  "branch": "main",
  "status": "QUEUED",
  "options": {
    "key": "value"
  },
  "created_at": "2026-01-27T10:30:00Z"
}
```

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `job_id` | string (UUID) | Unique job identifier |
| `repo_url` | string | Git repository URL |
| `branch` | string | Git branch name |
| `status` | string | Current job status |
| `options` | object | Custom analysis options |
| `created_at` | ISO 8601 timestamp | Job creation time |

### Repository

Represents a tracked repository in the system.

```json
{
  "id": 1,
  "url": "https://github.com/facebook/react.git",
  "owner_id": 1,
  "created_at": "2026-01-27T10:30:00Z"
}
```

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | integer | Unique repository identifier |
| `url` | string | Repository URL |
| `owner_id` | integer | Owner user ID |
| `created_at` | ISO 8601 timestamp | Repository creation time |

---

## Example Workflows

### Workflow 1: Analyze a Repository

1. **Create Analysis Job**
   ```bash
   curl -X POST http://localhost:8080/api/v1/analyze \
     -H "Content-Type: application/json" \
     -d '{
       "repo_url": "https://github.com/user/repo.git",
       "branch": "develop"
     }'
   ```

2. **Poll Job Status** (repeat until status changes)
   ```bash
   curl http://localhost:8080/api/v1/jobs/{job_id}
   ```

3. **Process Complete** - Status will change to `COMPLETED` or `FAILED`

### Workflow 2: Discover Analyzed Repositories

1. **List All Repositories**
   ```bash
   curl http://localhost:8080/api/v1/repositories
   ```

2. **Get Details of Specific Repository**
   ```bash
   curl http://localhost:8080/api/v1/repositories/{repo_id}
   ```

---

## Performance Considerations

- Job listing returns the **50 most recent jobs**
- Repository queries are **not paginated** - consider pagination if dataset grows large
- Redis is used for **queue-based job distribution** to support horizontal scaling
- PostgreSQL stores **job metadata and repository information**

---

## Development

### Running the API Gateway

```bash
cd apps/api-gateway
go run main.go
```

### Running the Graph Engine

```bash
cd services/graph-engine
pip install -r requirements.txt
python main.py
```

Or with uvicorn directly:
```bash
uvicorn main:app --reload --host 0.0.0.0 --port 8000
```

### API Gateway Dependencies

The API Gateway uses the following Go packages:

- `github.com/gin-gonic/gin` - HTTP framework
- `github.com/gin-contrib/cors` - CORS middleware
- `github.com/go-redis/redis/v8` - Redis client
- `github.com/lib/pq` - PostgreSQL driver
- `github.com/google/uuid` - UUID generation
- `github.com/joho/godotenv` - Environment variable loading

### Graph Engine Dependencies

The Graph Engine uses the following Python packages:

- `fastapi` - Modern async web framework
- `uvicorn` - ASGI server
- `neo4j` - Neo4j Python driver
- `networkx` - Graph analysis library (for PageRank)
- `pydantic` - Data validation
- `python-dotenv` - Environment variable loading

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.1 | 2026-02-03 | Added Graph Engine documentation (impact analysis, metrics, graph retrieval, PageRank, custom queries) |
| 1.0 | 2026-01-27 | Initial API documentation |

---

## Support

For issues or questions:
- Check [GITHUB_ISSUES.md](./GITHUB_ISSUES.md) for known issues
- Review architecture details in [README.md](../README.md)
- Check implementation details in [TREE_SITTER_IMPLEMENTATION.md](./TREE_SITTER_IMPLEMENTATION.md)
