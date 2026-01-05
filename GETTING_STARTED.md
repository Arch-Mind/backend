# 🚀 ArchMind - Quick Start Guide

Welcome to ArchMind! This guide will help you get the entire platform running locally.

## 📋 Prerequisites

Ensure you have the following installed:

- **Docker & Docker Compose** (for infrastructure)
- **Go 1.21+** (for API Gateway)
- **Rust 1.75+** (for Ingestion Worker)
- **Python 3.11+** (for Graph Engine)
- **Node.js 20+** and **pnpm** (for Web Dashboard)
- **Git**

## 🏃 Quick Start (5 Minutes)

### Step 1: Start Infrastructure Services

```bash
cd infra
docker-compose up -d
```

This starts:
- PostgreSQL on port 5432
- Neo4j on ports 7474 (HTTP) and 7687 (Bolt)
- Redis on port 6379
- MinIO on ports 9000 (API) and 9001 (Console)

**Verify services are healthy:**
```bash
docker-compose ps
```

### Step 2: Initialize Databases

**PostgreSQL:**
```bash
# Schema is auto-initialized on first startup
# Verify with:
docker exec -it archmind-postgres psql -U postgres -d archmind -c "\dt"
```

**Neo4j:**
```bash
# Open Neo4j Browser: http://localhost:7474
# Login: neo4j/password
# Run the initialization script from: infra/neo4j/init/001_schema.cypher
```

### Step 3: Start Backend Services

**Terminal 1 - API Gateway (Go):**
```bash
cd apps/api-gateway
cp .env.example .env
go mod download
go run main.go
```
→ Running on http://localhost:8080

**Terminal 2 - Ingestion Worker (Rust):**
```bash
cd services/ingestion-worker
cp .env.example .env
cargo build --release
cargo run --release
```
→ Listening for jobs on Redis queue

**Terminal 3 - Graph Engine (Python):**
```bash
cd services/graph-engine
cp .env.example .env
python -m venv venv
# On Windows:
venv\Scripts\activate
# On Linux/Mac:
source venv/bin/activate
pip install -r requirements.txt
uvicorn main:app --reload
```
→ Running on http://localhost:8000

### Step 4: Start Frontend

**Terminal 4 - Web Dashboard (Next.js):**
```bash
cd apps/web-dashboard
pnpm install
pnpm dev
```
→ Running on http://localhost:3000

## ✅ Verify Everything Works

### 1. Check Health Endpoints

```bash
# API Gateway
curl http://localhost:8080/health

# Graph Engine
curl http://localhost:8000/health
```

### 2. Submit a Test Repository

Open http://localhost:3000 and submit a repository URL, or use curl:

```bash
curl -X POST http://localhost:8080/api/v1/analyze \
  -H "Content-Type: application/json" \
  -d '{
    "repo_url": "https://github.com/rust-lang/rust",
    "branch": "master"
  }'
```

You should receive a response with a `job_id`.

### 3. Check Job Status

```bash
curl http://localhost:8080/api/v1/jobs/{job_id}
```

### 4. View in Neo4j Browser

Open http://localhost:7474 and run:

```cypher
MATCH (n) RETURN n LIMIT 25
```

## 📁 Project Structure

```
arch-mind/
├── apps/
│   ├── web-dashboard/       # Next.js frontend (port 3000)
│   ├── api-gateway/         # Go API (port 8080)
│   └── vscode-extension/    # VS Code extension (TBD)
├── services/
│   ├── ingestion-worker/    # Rust parser
│   └── graph-engine/        # Python analysis (port 8000)
├── packages/
│   └── shared-schemas/      # Shared TypeScript types
└── infra/
    ├── docker-compose.yml   # Infrastructure services
    ├── postgres/            # PostgreSQL schemas
    └── neo4j/              # Neo4j initialization
```

## 🔧 Development Workflows

### Adding New Features

1. Check [GITHUB_ISSUES.md](GITHUB_ISSUES.md) for planned features
2. Create a branch: `git checkout -b feature/your-feature`
3. Make changes in the appropriate service
4. Test locally
5. Submit a pull request

### Running Tests

```bash
# Go API Gateway
cd apps/api-gateway && go test ./...

# Rust Worker
cd services/ingestion-worker && cargo test

# Python Graph Engine
cd services/graph-engine && pytest

# Next.js Frontend
cd apps/web-dashboard && pnpm test
```

### Viewing Logs

```bash
# Docker services
docker-compose logs -f

# Specific service
docker-compose logs -f postgres
docker-compose logs -f neo4j
docker-compose logs -f redis
```

## 🌐 Service URLs

| Service | URL | Credentials |
|---------|-----|-------------|
| Web Dashboard | http://localhost:3000 | - |
| API Gateway | http://localhost:8080 | - |
| Graph Engine | http://localhost:8000 | - |
| API Docs | http://localhost:8000/docs | - |
| PostgreSQL | localhost:5432 | postgres/postgres |
| Neo4j Browser | http://localhost:7474 | neo4j/password |
| Neo4j Bolt | bolt://localhost:7687 | neo4j/password |
| Redis | localhost:6379 | - |
| MinIO Console | http://localhost:9001 | minioadmin/minioadmin |

## 🐛 Troubleshooting

### Port Already in Use

```bash
# Find process using port
netstat -ano | findstr :8080  # Windows
lsof -i :8080                 # Mac/Linux

# Kill process
taskkill /PID <pid> /F        # Windows
kill -9 <pid>                 # Mac/Linux
```

### Docker Services Won't Start

```bash
# Reset everything
docker-compose down -v
docker-compose up -d

# Check logs
docker-compose logs
```

### Go Dependencies Issue

```bash
cd apps/api-gateway
go clean -modcache
go mod download
```

### Rust Compilation Errors

```bash
cd services/ingestion-worker
cargo clean
cargo build --release
```

### Python Package Issues

```bash
cd services/graph-engine
rm -rf venv
python -m venv venv
source venv/bin/activate  # or venv\Scripts\activate on Windows
pip install -r requirements.txt
```

### Next.js Build Errors

```bash
cd apps/web-dashboard
rm -rf .next node_modules
pnpm install
pnpm dev
```

## 📚 Next Steps

1. Read the [main README.md](README.md) for detailed architecture
2. Check [GITHUB_ISSUES.md](GITHUB_ISSUES.md) for planned features
3. Explore individual service READMEs:
   - [apps/api-gateway/README.md](apps/api-gateway/README.md)
   - [services/ingestion-worker/README.md](services/ingestion-worker/README.md)
   - [services/graph-engine/README.md](services/graph-engine/README.md)
   - [apps/web-dashboard/README.md](apps/web-dashboard/README.md)
4. Review [infra/README.md](infra/README.md) for infrastructure details
5. Check [infra/postgres/README.md](infra/postgres/README.md) for database info

## 🤝 Contributing

We welcome contributions! See our planned issues in [GITHUB_ISSUES.md](GITHUB_ISSUES.md).

Quick wins for first-time contributors:
- Issue #31: Add loading spinners
- Issue #32: Improve error messages
- Issue #33: Add dark mode toggle
- Issue #34: Add repository URL validation
- Issue #35: Create contributing guidelines

## 📞 Support

- 📖 Documentation: Check service-specific READMEs
- 🐛 Bug Reports: Create an issue on GitHub
- 💡 Feature Requests: Check GITHUB_ISSUES.md first
- 💬 Discussions: Use GitHub Discussions

## 🎉 Success!

If all services are running, you've successfully set up ArchMind! 🎊

Try analyzing your first repository and explore the graph visualization.

---

**Happy Coding! 🚀**
