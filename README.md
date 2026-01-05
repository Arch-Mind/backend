# ArchMind - Real-Time Codebase Intelligence & Architecture Reconstruction Platform

> A polyglot source code analysis platform that builds dependency graphs and visualizes architecture with real-time impact analysis.

## 🎯 Overview

ArchMind parses source code across multiple languages, constructs comprehensive dependency graphs, and provides real-time visualization of architecture and impact analysis. Built as a modular monolith with event-driven async workers.

## 🏗️ Architecture

- **Architecture Pattern**: Modular Monolith with Event-Driven Async Workers
- **Repository Style**: Monorepo
- **Communication**: Event-driven via Redis message broker

## 🛠️ Tech Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| **Frontend** | Next.js, Tailwind CSS, react-force-graph | Web dashboard with WebGL visualization |
| **API Gateway** | Go (Gin) | Authentication, webhooks, job orchestration |
| **Ingestion Worker** | Rust | Heavy-duty parsing with tree-sitter and git operations |
| **Graph Engine** | Python (FastAPI) | Graph algorithms with networkx and Neo4j |
| **Graph Database** | Neo4j | Dependency graph storage (nodes/edges) |
| **Relational DB** | PostgreSQL | User, project, and job metadata |
| **Message Broker** | Redis | Job queue and pub/sub |
| **IDE Extension** | TypeScript (VS Code) | In-editor integration |

## 📁 Project Structure

```
arch-mind/
├── .github/                      # CI/CD workflows
├── infra/                        # Infrastructure configuration
│   ├── docker-compose.yml        # Local development stack
│   ├── postgres/                 # PostgreSQL migrations
│   └── neo4j/                    # Neo4j initialization scripts
├── apps/
│   ├── web-dashboard/            # Next.js frontend application
│   ├── api-gateway/              # Go API gateway service
│   └── vscode-extension/         # VS Code extension
├── services/
│   ├── ingestion-worker/         # Rust parsing worker
│   └── graph-engine/             # Python graph analysis service
└── packages/
    └── shared-schemas/           # Shared types and schemas
```

## 🚀 Quick Start

### Prerequisites

- Docker & Docker Compose
- Go 1.21+
- Rust 1.75+
- Python 3.11+
- Node.js 20+
- pnpm (or npm/yarn)

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

7. **Start Web Dashboard (Next.js)**
   ```bash
   cd apps/web-dashboard
   pnpm install
   pnpm dev
   ```

### Service URLs

- **Web Dashboard**: http://localhost:3000
- **API Gateway**: http://localhost:8080
- **Graph Engine**: http://localhost:8000
- **Neo4j Browser**: http://localhost:7474 (neo4j/password)
- **PostgreSQL**: localhost:5432 (postgres/postgres)
- **Redis**: localhost:6379

## 📊 Data Models

### Neo4j Graph Schema

**Nodes:**
- `File` - Source code files
- `Function` - Function/method definitions
- `Class` - Class definitions
- `Module` - Module/package definitions

**Relationships:**
- `CALLS` - Function invocation
- `IMPORTS` - Import/dependency
- `INHERITS` - Class inheritance
- `CONTAINS` - Containment relationship

### PostgreSQL Schema

**Tables:**
- `users` - User accounts (GitHub OAuth)
- `repositories` - Tracked repositories
- `analysis_jobs` - Job queue and status tracking

## 🔄 Workflow

1. User submits repository URL via web dashboard
2. API Gateway authenticates and creates analysis job
3. Job pushed to Redis queue
4. Rust Ingestion Worker picks up job:
   - Clones repository
   - Parses code with tree-sitter
   - Extracts dependencies
5. Worker pushes graph data to Neo4j
6. Python Graph Engine processes:
   - Runs graph algorithms (PageRank, centrality, etc.)
   - Detects architectural patterns
   - Identifies hotspots and impact zones
7. Results displayed in real-time on web dashboard

## 🧩 API Endpoints

### API Gateway (Go)

- `POST /api/v1/analyze` - Submit repository for analysis
- `GET /api/v1/jobs/:id` - Get job status
- `GET /api/v1/repositories` - List tracked repositories
- `POST /api/v1/auth/github` - GitHub OAuth callback

### Graph Engine (Python)

- `GET /api/impact/:node` - Impact analysis for a node
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
GITHUB_CLIENT_ID=your_client_id
GITHUB_CLIENT_SECRET=your_client_secret
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
# Run all tests
./scripts/test-all.sh

# Individual service tests
cd apps/api-gateway && go test ./...
cd services/ingestion-worker && cargo test
cd services/graph-engine && pytest
cd apps/web-dashboard && pnpm test
```

## 📦 Deployment

Docker images are built for each service:

```bash
# Build all images
docker-compose -f docker-compose.prod.yml build

# Deploy to Kubernetes
kubectl apply -f k8s/
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [tree-sitter](https://tree-sitter.github.io/) - Parsing framework
- [Neo4j](https://neo4j.com/) - Graph database
- [react-force-graph](https://github.com/vasturiano/react-force-graph) - WebGL visualization
