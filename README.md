# ArchMind - Backend: Code Analysis & Dependency Graph Platform

> A polyglot backend platform for parsing JavaScript/TypeScript source code, building dependency graphs, and enabling real-time architecture analysis.

## 🎯 Overview

ArchMind is a backend-only platform that parses JavaScript/TypeScript codebases using Tree-sitter, extracts function definitions and calls, constructs comprehensive dependency graphs, and stores them in Neo4j for querying and analysis. Built as a modular event-driven architecture with async workers.

## 🚀 Quick Start with Docker

The fastest way to run ArchMind is using Docker Compose:

```bash
# 1. Navigate to backend directory
cd backend

# 2. Set up environment variables (copy and edit)
cp services/graph-engine/.env.example services/graph-engine/.env
# Edit .env to add your GEMINI_API_KEY

# 3. Start all services
docker-compose up --build

# 4. Run health checks
bash healthcheck.sh

# 5. Run database migrations
docker exec archmind-postgres psql -U postgres -d arch-mind < infra/postgres/init/001_schema.sql
docker exec archmind-postgres psql -U postgres -d arch-mind < infra/postgres/init/002_file_contributions.sql
docker exec archmind-postgres psql -U postgres -d arch-mind < infra/postgres/init/003_architecture_insights.sql
```

**Services available at:**
- API Gateway: http://localhost:8080
- Graph Engine: http://localhost:8000
- Neo4j Browser: http://localhost:7474
- PostgreSQL: localhost:5432
- Redis: localhost:6379

For production deployment to Railway, see [RAILWAY_DEPLOYMENT.md](RAILWAY_DEPLOYMENT.md)

## 🏗️ Architecture

### High-Level Overview

ArchMind follows a **microservices architecture** with **event-driven async workers**. The system is designed for scalability, language extensibility, and real-time code analysis.

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Client Applications                          │
│                     (API clients, CLI tools, etc.)                   │
└────────────────────────────────┬────────────────────────────────────┘
                                 │ HTTP/REST
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          API Gateway (Go)                            │
│  • REST API endpoints                                                │
│  • Job creation & orchestration                                      │
│  • Authentication & validation                                       │
│  • PostgreSQL for metadata storage                                   │
└──────────────────────┬──────────────────────────────────────────────┘
                       │
                       │ Push job to queue
                       ▼
         ┌─────────────────────────────┐
         │      Redis Message Broker    │
         │  • Job queue (BRPOP)         │
         │  • Pub/Sub for events        │
         └────────┬────────────────────┘
                  │
                  │ Pop job (blocking)
                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    Ingestion Worker (Rust)                           │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │ 1. Clone Repository (git2)                                    │  │
│  │ 2. Walk Directory Tree                                        │  │
│  │ 3. Parse Files (Tree-sitter)                                  │  │
│  │    • JavaScript Parser                                        │  │
│  │    • TypeScript Parser                                        │  │
│  │    • (Extensible: Rust, Go, Python parsers)                   │  │
│  │ 4. Extract AST Data                                           │  │
│  │    • Functions (name, location, calls)                        │  │
│  │    • Imports (dependencies)                                   │  │
│  │ 5. Build Dependency Graph                                     │  │
│  │ 6. Store in Neo4j                                             │  │
│  └───────────────────────────────────────────────────────────────┘  │
└──────────────────────┬──────────────────────────────────────────────┘
                       │
                       │ Cypher queries
                       ▼
         ┌─────────────────────────────┐
         │      Neo4j Graph Database    │
         │  Nodes: File, Function,      │
         │         Module, Job          │
         │  Edges: DEFINES, CALLS,      │
         │         IMPORTS              │
         └────────┬────────────────────┘
                  │
                  │ Graph queries & analysis
                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     Graph Engine (Python)                            │
│  • FastAPI REST endpoints                                            │
│  • Graph algorithms (NetworkX)                                       │
│    - PageRank for important functions                                │
│    - Shortest path analysis                                          │
│    - Circular dependency detection                                   │
│    - Community detection (module clusters)                           │
│  • Impact analysis (what depends on X?)                              │
│  • Complexity metrics                                                │
└─────────────────────────────────────────────────────────────────────┘
```

### Architecture Principles

- **Separation of Concerns**: Each service has a single responsibility
- **Event-Driven**: Async processing via message queues
- **Polyglot**: Right tool for the job (Go for API, Rust for parsing, Python for analytics)
- **Scalability**: Workers can be horizontally scaled
- **Language Agnostic**: Parser architecture supports multiple languages

## 🛠️ Tech Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| **API Gateway** | Go (Gin) | REST API, webhooks, job orchestration |
| **Ingestion Worker** | Rust + Tree-sitter | JS/TS parsing, AST extraction, git operations |
| **Graph Engine** | Python (FastAPI) | Graph algorithms with networkx and Neo4j |
| **Graph Database** | Neo4j | Dependency graph storage (nodes/edges) |
| **Relational DB** | PostgreSQL | User, project, and job metadata |
| **Message Broker** | Redis | Job queue and pub/sub |

## 📁 Project Structure

```
arch-mind/
├── .github/                      # CI/CD workflows
├── infra/                        # Infrastructure configuration
│   ├── docker-compose.yml        # Local development stack
│   ├── postgres/                 # PostgreSQL migrations
│   └── neo4j/                    # Neo4j initialization scripts
├── apps/
│   └── api-gateway/              # Go API gateway service
└── services/
    ├── ingestion-worker/         # Rust JS/TS parsing worker with Tree-sitter
    └── graph-engine/             # Python graph analysis service
```

## ✨ Features

### Tree-sitter JavaScript/TypeScript Integration ✅

The Ingestion Worker now includes full Tree-sitter integration for parsing JavaScript and TypeScript:

- **Function Detection**: Identifies function declarations, arrow functions, and method definitions
- **Call Graph Extraction**: Maps which functions call other functions
- **Import Analysis**: Tracks module dependencies and import statements
- **Multi-file Analysis**: Walks entire repositories and builds cross-file dependency graphs
- **Neo4j Storage**: Stores parsed AST data as graph nodes and relationships

#### Supported Syntax:
- Function declarations: `function greet() {}`
- Arrow functions: `const add = (a, b) => a + b`
- Method definitions: `class MyClass { myMethod() {} }`
- ES6 imports: `import { foo } from 'bar'`
- CommonJS requires: `const axios = require('axios')`

---

## 🔄 Detailed Backend Workflow

### 1. Job Submission (API Gateway)

**Endpoint**: `POST /api/v1/analyze`

```json
{
  "repo_url": "https://github.com/user/repo.git",
  "branch": "main",
  "options": {
    "deep_analysis": true
  }
}
```

**Process**:
1. API Gateway validates the request
2. Creates a job record in PostgreSQL with status `PENDING`
3. Generates unique job ID (UUID)
4. Serializes job data to JSON
5. Pushes to Redis queue: `LPUSH analysis_queue <job_json>`
6. Returns job ID to client

**PostgreSQL Schema**:
```sql
CREATE TABLE analysis_jobs (
    id UUID PRIMARY KEY,
    repo_url TEXT NOT NULL,
    branch TEXT NOT NULL,
    status VARCHAR(20) NOT NULL, -- PENDING, PROCESSING, COMPLETED, FAILED
    created_at TIMESTAMP DEFAULT NOW(),
    completed_at TIMESTAMP,
    options JSONB
);
```

---

### 2. Job Processing (Ingestion Worker)

The Rust worker runs in an infinite loop, blocking on Redis queue.

**Main Loop**:
```rust
loop {
    // Blocking pop with 5 second timeout
    let job = redis.brpop("analysis_queue", 5.0).await;
    
    if let Some(job_data) = job {
        process_job(job_data).await;
    }
}
```

#### Step 2.1: Repository Cloning

```rust
fn clone_repository(repo_url: &str, branch: &str) -> Result<PathBuf> {
    let tmp_dir = env::temp_dir().join(format!("repo_{}", Uuid::new_v4()));
    let repo = Repository::clone(repo_url, &tmp_dir)?;
    repo.set_head(&format!("refs/heads/{}", branch))?;
    Ok(tmp_dir)
}
```

- Uses `git2` library for Git operations
- Clones to temporary directory
- Checks out specified branch

#### Step 2.2: Directory Walking

```rust
fn walk_directory(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        
        // Skip ignored directories
        if is_ignored(&path) { continue; }
        
        if path.is_dir() {
            files.extend(walk_directory(&path)?);
        } else if is_parseable(&path) {
            files.push(path);
        }
    }
    
    Ok(files)
}
```

**Ignored Patterns**:
- `node_modules/`, `.git/`, `dist/`, `build/`, `target/`
- Hidden files (starting with `.`)

**Parseable Extensions**:
- `.js`, `.jsx`, `.mjs` → JavaScript Parser
- `.ts`, `.tsx` → TypeScript Parser

#### Step 2.3: Tree-sitter Parsing

For each file, the appropriate parser is invoked:

```rust
pub trait LanguageParser {
    fn parse_file(&self, path: &Path, content: &str) -> Result<ParsedFile>;
}

pub struct JavaScriptParser {
    parser: tree_sitter::Parser,
}

impl LanguageParser for JavaScriptParser {
    fn parse_file(&self, path: &Path, content: &str) -> Result<ParsedFile> {
        // 1. Parse source code to AST
        let tree = self.parser.parse(content, None)?;
        
        // 2. Query for functions
        let functions = extract_functions(&tree, content)?;
        
        // 3. Query for function calls
        let calls = extract_calls(&tree, content)?;
        
        // 4. Query for imports
        let imports = extract_imports(&tree, content)?;
        
        Ok(ParsedFile { path, functions, calls, imports })
    }
}
```

**Tree-sitter Queries**:

```scheme
; Function declarations
(function_declaration
  name: (identifier) @func.name) @func.def

; Arrow functions
(variable_declarator
  name: (identifier) @func.name
  value: (arrow_function)) @func.def

; Function calls
(call_expression
  function: (identifier) @call.name)

; Imports
(import_statement
  source: (string) @import.source)
```

**Output Structure**:
```rust
pub struct ParsedFile {
    pub path: String,
    pub language: String,
    pub functions: Vec<FunctionInfo>,
    pub imports: Vec<String>,
}

pub struct FunctionInfo {
    pub name: String,
    pub calls: Vec<String>,      // Functions called within this function
    pub start_line: usize,
    pub end_line: usize,
}
```

#### Step 2.4: Dependency Extraction

```rust
fn extract_dependencies(files: &[ParsedFile]) -> Vec<Dependency> {
    let mut deps = Vec::new();
    
    // Build function lookup map
    let mut func_map = HashMap::new();
    for file in files {
        for func in &file.functions {
            func_map.insert(&func.name, &file.path);
        }
    }
    
    // Map calls to dependencies
    for file in files {
        for func in &file.functions {
            for call in &func.calls {
                if let Some(target_file) = func_map.get(call) {
                    deps.push(Dependency {
                        from: format!("{}::{}", file.path, func.name),
                        to: format!("{}::{}", target_file, call),
                        relationship_type: "CALLS",
                    });
                }
            }
        }
    }
    
    deps
}
```

#### Step 2.5: Neo4j Storage

The worker creates a comprehensive graph structure:

```rust
async fn store_in_neo4j(
    graph: &Graph,
    job_id: &str,
    files: &[ParsedFile],
    deps: &[Dependency],
) -> Result<()> {
    // Create Job node
    graph.run(query("CREATE (j:Job {id: $id})").param("id", job_id)).await?;
    
    // Create File nodes
    for file in files {
        graph.run(query(
            "MERGE (f:File {path: $path}) SET f.language = $lang"
        ).param("path", &file.path).param("lang", &file.language)).await?;
        
        // Create Function nodes and DEFINES relationships
        for func in &file.functions {
            graph.run(query(
                "MERGE (fn:Function {name: $name, file: $file})
                 SET fn.start_line = $start, fn.end_line = $end"
            ).param("name", &func.name)
             .param("file", &file.path)
             .param("start", func.start_line)
             .param("end", func.end_line)).await?;
            
            graph.run(query(
                "MATCH (f:File {path: $file})
                 MATCH (fn:Function {name: $func, file: $file})
                 MERGE (f)-[:DEFINES]->(fn)"
            ).param("file", &file.path).param("func", &func.name)).await?;
        }
    }
    
    // Create CALLS relationships
    for dep in deps {
        graph.run(query(
            "MATCH (from:Function {name: $from_func})
             MATCH (to:Function {name: $to_func})
             MERGE (from)-[:CALLS]->(to)"
        ).param("from_func", &dep.from).param("to_func", &dep.to)).await?;
    }
    
    Ok(())
}
```

**Neo4j Graph Schema**:
```cypher
// Nodes
(:Job {id: UUID, status: STRING, timestamp: DATETIME})
(:File {path: STRING, language: STRING})
(:Function {name: STRING, file: STRING, start_line: INT, end_line: INT})
(:Module {name: STRING})

// Relationships
(File)-[:DEFINES]->(Function)
(Function)-[:CALLS]->(Function)
(File)-[:IMPORTS]->(Module)
```

---

### 3. Graph Analysis (Graph Engine)

The Python service provides analytical queries over the graph.

#### Example 1: Impact Analysis

**Query**: "What will break if I change function X?"

```python
@app.get("/api/impact/{function_name}")
async def get_impact_analysis(function_name: str):
    query = """
    MATCH path = (fn:Function {name: $name})<-[:CALLS*1..5]-(caller)
    RETURN DISTINCT caller.name AS affected_function,
                    caller.file AS file,
                    length(path) AS depth
    ORDER BY depth
    """
    
    result = await neo4j.run(query, {"name": function_name})
    return {"impacted_functions": result.records}
```

#### Example 2: Circular Dependencies

```python
@app.get("/api/circular-deps/{repo}")
async def find_circular_deps(repo: str):
    query = """
    MATCH path = (f1:Function)-[:CALLS*2..10]->(f1)
    WHERE f1.file STARTS WITH $repo
    RETURN [n IN nodes(path) | n.name] AS cycle
    """
    
    cycles = await neo4j.run(query, {"repo": repo})
    return {"circular_dependencies": cycles.records}
```

#### Example 3: Function Importance (PageRank)

```python
@app.get("/api/metrics/important-functions")
async def get_important_functions():
    # Export graph to NetworkX
    G = nx.DiGraph()
    
    # Load from Neo4j
    result = await neo4j.run("""
        MATCH (from:Function)-[:CALLS]->(to:Function)
        RETURN from.name AS source, to.name AS target
    """)
    
    for record in result:
        G.add_edge(record["source"], record["target"])
    
    # Run PageRank
    pagerank = nx.pagerank(G)
    
    # Sort by importance
    important = sorted(pagerank.items(), key=lambda x: x[1], reverse=True)
    
    return {"important_functions": important[:20]}
```

---

## 🗄️ Data Flow Example

Let's trace a complete example:

### Input Repository

```
my-app/
├── src/
│   ├── api/
│   │   └── users.js
│   └── utils/
│       └── validation.js
└── package.json
```

**File: `src/api/users.js`**
```javascript
import axios from 'axios';
import { validateUser } from '../utils/validation';

async function fetchUsers() {
    const response = await axios.get('/api/users');
    return processUsers(response.data);
}

function processUsers(data) {
    return data.filter(user => validateUser(user));
}

export { fetchUsers, processUsers };
```

**File: `src/utils/validation.js`**
```javascript
export function validateUser(user) {
    return user && user.email && user.name;
}
```

### Parsed Output

**Ingestion Worker extracts**:

```rust
ParsedFile {
    path: "src/api/users.js",
    language: "javascript",
    functions: [
        FunctionInfo { 
            name: "fetchUsers", 
            calls: ["get", "processUsers"],
            start_line: 4, 
            end_line: 7 
        },
        FunctionInfo { 
            name: "processUsers", 
            calls: ["filter", "validateUser"],
            start_line: 9, 
            end_line: 11 
        }
    ],
    imports: ["axios", "../utils/validation"]
}

ParsedFile {
    path: "src/utils/validation.js",
    language: "javascript",
    functions: [
        FunctionInfo { 
            name: "validateUser", 
            calls: [],
            start_line: 1, 
            end_line: 3 
        }
    ],
    imports: []
}
```

### Neo4j Graph Created

```cypher
// Files
CREATE (f1:File {path: "src/api/users.js", language: "javascript"})
CREATE (f2:File {path: "src/utils/validation.js", language: "javascript"})

// Functions
CREATE (fn1:Function {name: "fetchUsers", file: "src/api/users.js", start_line: 4, end_line: 7})
CREATE (fn2:Function {name: "processUsers", file: "src/api/users.js", start_line: 9, end_line: 11})
CREATE (fn3:Function {name: "validateUser", file: "src/utils/validation.js", start_line: 1, end_line: 3})

// Modules
CREATE (m1:Module {name: "axios"})
CREATE (m2:Module {name: "../utils/validation"})

// Relationships
CREATE (f1)-[:DEFINES]->(fn1)
CREATE (f1)-[:DEFINES]->(fn2)
CREATE (f2)-[:DEFINES]->(fn3)
CREATE (fn1)-[:CALLS]->(fn2)
CREATE (fn2)-[:CALLS]->(fn3)
CREATE (f1)-[:IMPORTS]->(m1)
CREATE (f1)-[:IMPORTS]->(m2)
```

### Query Results

**Impact Analysis**: "What depends on `validateUser`?"

```cypher
MATCH (fn:Function {name: "validateUser"})<-[:CALLS*]-(caller)
RETURN caller.name

Results:
- processUsers
- fetchUsers (indirect, via processUsers)
```

**Visualization**:
```
axios ←─── fetchUsers ──→ processUsers ──→ validateUser
(module)      (fn)             (fn)            (fn)
```

## 🚀 Quick Start

### Prerequisites

- Docker & Docker Compose
- Go 1.21+
- Rust 1.75+
- Python 3.11+

### Local Development Setup

1. **Clone the repository**
   ```bash
   git clone https://github.com/yourusername/arch-mind.git
   cd arch-mind
   ```

2. **Start infrastructure services**
   ```bash
   cd infra
   docker-compose up -d
   ```

3. **Initialize databases**
   ```bash
   # PostgreSQL migrations
   cd infra/postgres
   ./run-migrations.sh

   # Neo4j setup (runs automatically on first start)
   ```

4. **Start API Gateway (Go)**
   ```bash
   cd apps/api-gateway
   go mod download
   go run main.go
   ```

5. **Start Ingestion Worker (Rust)**
   ```bash
   cd services/ingestion-worker
   cargo build --release
   cargo run
   ```

6. **Start Graph Engine (Python)**
   ```bash
   cd services/graph-engine
   pip install -r requirements.txt
   uvicorn main:app --reload
   ```


### Service URLs

- **API Gateway**: http://localhost:8080
- **Graph Engine**: http://localhost:8000
- **Neo4j Browser**: http://localhost:7474 (neo4j/password)
- **PostgreSQL**: localhost:5432 (postgres/postgres)
- **Redis**: localhost:6379

## 📊 Data Models

### Neo4j Graph Schema

**Nodes:**
- `Job` - Analysis job metadata
- `File` - Source code files (JS/TS)
- `Function` - Function/method definitions
- `Module` - External module dependencies

**Relationships:**
- `DEFINES` - File defines function
- `CALLS` - Function invocation
- `IMPORTS` - Module import dependency

### PostgreSQL Schema

**Tables:**
- `repositories` - Tracked repositories
- `analysis_jobs` - Job queue and status tracking
- `users` - User accounts (optional)

## 🔄 Workflow

1. Submit repository URL to API Gateway
2. API Gateway creates analysis job and pushes to Redis queue
3. Rust Ingestion Worker picks up job:
   - Clones repository from Git
   - Walks directory tree for .js/.ts files
   - Parses code with Tree-sitter
   - Extracts function definitions, calls, and imports
   - Builds dependency graph
4. Worker stores graph data in Neo4j:
   - Creates File, Function, and Module nodes
   - Creates DEFINES, CALLS, and IMPORTS relationships
5. Python Graph Engine can query and analyze:
   - Run graph algorithms (PageRank, centrality)
   - Detect circular dependencies
   - Identify hotspots and impact zones
6. Results available via API queries

## 🧩 API Endpoints

### API Gateway (Go)

- `POST /api/v1/analyze` - Submit repository for analysis
- `GET /api/v1/jobs/:id` - Get job status
- `GET /api/v1/repositories` - List tracked repositories

### Graph Engine (Python)

- `GET /api/impact/:function` - Impact analysis for a function
- `GET /api/metrics/:repo` - Repository metrics
- `GET /api/graph/:repo` - Full dependency graph
- `POST /api/query` - Custom Cypher query execution

## 🔐 Environment Variables

Create `.env` files in each service directory:

**API Gateway:**
```env
PORT=8080
REDIS_URL=redis://localhost:6379
POSTGRES_URL=postgresql://postgres:postgres@localhost:5432/archmind
```

**Ingestion Worker:**
```env
REDIS_URL=redis://localhost:6379
NEO4J_URI=bolt://localhost:7687
NEO4J_USER=neo4j
NEO4J_PASSWORD=password
```

**Graph Engine:**
```env
NEO4J_URI=bolt://localhost:7687
NEO4J_USER=neo4j
NEO4J_PASSWORD=password
```

## 🧪 Testing

```bash
# Individual service tests
cd apps/api-gateway && go test ./...
cd services/ingestion-worker && cargo test
cd services/graph-engine && pytest
```

## � Documentation

Comprehensive documentation is available in the [`docs/`](docs/) folder:

- **[Tree-sitter Implementation](docs/TREE_SITTER_IMPLEMENTATION.md)** - Detailed guide on JavaScript/TypeScript parsing with Tree-sitter
- **[Getting Started](docs/GETTING_STARTED.md)** - Step-by-step setup instructions
- **[GitHub Issues](docs/GITHUB_ISSUES.md)** - Issue tracking and development roadmap

## 📦 Deployment

Docker images are built for each service:

```bash
# Build all images
docker-compose -f docker-compose.prod.yml build

# Deploy to Kubernetes
kubectl apply -f k8s/
```

## 🔧 Performance & Scalability

### Horizontal Scaling

- **Ingestion Workers**: Multiple workers can pull from the same Redis queue
- **Graph Engine**: Stateless API, can run multiple instances behind a load balancer
- **API Gateway**: Horizontally scalable with shared PostgreSQL backend

### Optimization Strategies

1. **Incremental Parsing**: Only parse changed files (future enhancement)
2. **Parallel Processing**: Use Rayon to parse files in parallel
3. **Neo4j Indexing**: Create indexes on frequently queried properties
4. **Redis Pipelining**: Batch Neo4j writes for better throughput
5. **Caching**: Cache parsed results keyed by commit hash

### Typical Performance

- **Parsing Speed**: ~500-1000 files/second (Rust + Tree-sitter)
- **Graph Ingestion**: ~10,000 nodes/second to Neo4j
- **Query Latency**: <100ms for most graph queries
- **Repo Analysis**: Small repo (100 files) in ~2-5 seconds

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [tree-sitter](https://tree-sitter.github.io/) - Incremental parsing framework
- [Neo4j](https://neo4j.com/) - Graph database platform
- [Tokio](https://tokio.rs/) - Async runtime for Rust
- [FastAPI](https://fastapi.tiangolo.com/) - Python web framework
- [NetworkX](https://networkx.org/) - Graph analysis library
