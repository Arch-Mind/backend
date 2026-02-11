# Docker & Railway Deployment Setup - Summary

## ✅ What Was Created

Your backend is now fully containerized and ready for deployment! Here's what was set up:

### 📦 Docker Files

1. **Individual Service Dockerfiles**
   - `apps/api-gateway/Dockerfile` - Go API Gateway (multi-stage build)
   - `services/graph-engine/Dockerfile` - Python FastAPI service
   - `services/ingestion-worker/Dockerfile` - Rust worker (multi-stage build)

2. **Orchestration**
   - `docker-compose.yml` - Full stack with 6 services (postgres, neo4j, redis, api-gateway, graph-engine, ingestion-worker)
   - Includes health checks, volume persistence, and service dependencies

3. **Configuration Files**
   - `.dockerignore` - Excludes unnecessary files from Docker builds
   - `.env.railway.example` - Environment variable template for Railway
   - `railway.json` - Railway platform configuration

4. **Scripts & Documentation**
   - `healthcheck.ps1` - Windows PowerShell health check script
   - `healthcheck.sh` - Linux/Mac bash health check script
   - `quick-start.ps1` - Automated setup script for Windows
   - `RAILWAY_DEPLOYMENT.md` - Comprehensive deployment guide (300+ lines)

### 🎯 Key Features

- **Multi-stage builds** for API Gateway and Ingestion Worker (smaller images)
- **Health checks** for all services with proper retry logic
- **Volume persistence** for databases (data survives container restarts)
- **Service dependencies** with wait conditions
- **Environment variable injection** from host or .env files
- **Port mappings** for local development access

## 🚀 How to Use

### Option 1: Quick Start (Recommended for Windows)

```powershell
cd backend
.\quick-start.ps1
```

This automated script will:
1. Check if Docker is running
2. Create .env file from example (if missing)
3. Build all Docker images
4. Start all services
5. Run health checks
6. Apply database migrations

### Option 2: Manual Setup

```powershell
# 1. Set up environment
cd backend
cp services\graph-engine\.env.example services\graph-engine\.env
# Edit .env and add your GEMINI_API_KEY

# 2. Build and start services
docker-compose up --build -d

# 3. Wait 30 seconds for services to start
Start-Sleep -Seconds 30

# 4. Run health checks
.\healthcheck.ps1

# 5. Apply database migrations
Get-Content infra\postgres\init\001_schema.sql | docker exec -i archmind-postgres psql -U postgres -d arch-mind
Get-Content infra\postgres\init\002_file_contributions.sql | docker exec -i archmind-postgres psql -U postgres -d arch-mind
Get-Content infra\postgres\init\003_architecture_insights.sql | docker exec -i archmind-postgres psql -U postgres -d arch-mind
```

### Option 3: Deploy to Railway

Follow the comprehensive guide in [RAILWAY_DEPLOYMENT.md](RAILWAY_DEPLOYMENT.md)

**Quick Railway deployment:**

```bash
# Install Railway CLI
npm install -g @railway/cli

# Login
railway login

# Create project
railway init

# Deploy each service (detailed steps in RAILWAY_DEPLOYMENT.md)
```

## 📊 Service Architecture

```
┌─────────────────────────────────────────────────────┐
│                   Docker Host                        │
│                                                      │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐   │
│  │ PostgreSQL │  │   Neo4j    │  │   Redis    │   │
│  │   :5432    │  │ :7474,7687 │  │   :6379    │   │
│  └────────────┘  └────────────┘  └────────────┘   │
│         │              │               │            │
│         └──────────────┴───────────────┘            │
│                        │                            │
│  ┌─────────────────────┴──────────────────┐        │
│  │                                         │        │
│  │  ┌─────────────┐  ┌──────────────┐    │        │
│  │  │ API Gateway │  │ Graph Engine │    │        │
│  │  │    :8080    │  │    :8000     │    │        │
│  │  └─────────────┘  └──────────────┘    │        │
│  │         │                 │            │        │
│  │         └────────┬────────┘            │        │
│  │                  │                     │        │
│  │         ┌────────┴────────┐            │        │
│  │         │ Ingestion Worker│            │        │
│  │         │  (background)   │            │        │
│  │         └─────────────────┘            │        │
│  └──────────────────────────────────────┘         │
│                                                     │
└─────────────────────────────────────────────────────┘
```

## 🔍 Verification Steps

### 1. Check Service Health

```powershell
# Run automated health check
.\healthcheck.ps1

# Manual checks
curl http://localhost:8080/health
curl http://localhost:8000/health
```

### 2. View Service Logs

```powershell
# All services
docker-compose logs -f

# Specific service
docker-compose logs -f api-gateway
docker-compose logs -f graph-engine
docker-compose logs -f ingestion-worker
```

### 3. Test API Endpoints

```powershell
# Create a repository analysis job
curl -X POST http://localhost:8080/api/jobs `
  -H "Content-Type: application/json" `
  -d '{\"repo_url\": \"https://github.com/example/repo\"}'

# Get architecture insights (requires repo_id from ingestion)
curl http://localhost:8000/api/analyze/{repo_id}/architecture
```

### 4. Access Databases

```powershell
# PostgreSQL
docker exec -it archmind-postgres psql -U postgres -d arch-mind

# Neo4j Browser
# Open http://localhost:7474 in browser
# Credentials: neo4j / neo4j123

# Redis CLI
docker exec -it archmind-redis redis-cli
```

## 🛠️ Common Commands

```powershell
# Start services
docker-compose up -d

# Stop services
docker-compose down

# Rebuild and restart
docker-compose up --build -d

# View running containers
docker ps

# Remove all data (fresh start)
docker-compose down -v

# Scale ingestion workers
docker-compose up -d --scale ingestion-worker=3
```

## 📈 Performance Notes

### Image Sizes (Optimized with Multi-stage Builds)

- API Gateway: ~30 MB (Alpine-based)
- Graph Engine: ~250 MB (Python slim with minimal deps)
- Ingestion Worker: ~40 MB (Static Rust binary)

### Resource Requirements

**Minimum (Development):**
- RAM: 4 GB
- CPU: 2 cores
- Disk: 10 GB

**Recommended (Production):**
- RAM: 8 GB
- CPU: 4 cores
- Disk: 50 GB (for Neo4j graph storage)

## 🚨 Troubleshooting

### Services Won't Start

```powershell
# Check Docker daemon
docker info

# Check for port conflicts
netstat -ano | findstr "8080|8000|5432|7474|7687|6379"

# View detailed logs
docker-compose logs
```

### Database Connection Errors

- Ensure `POSTGRES_URL` has URL-encoded password: `venkat%2A2005`
- Wait 30 seconds after `docker-compose up` for dbs to initialize
- Check database logs: `docker-compose logs postgres neo4j`

### Ingestion Worker Not Processing Jobs

```powershell
# Check Redis queue
docker exec -it archmind-redis redis-cli LLEN jobs:analysis

# Check worker logs
docker-compose logs ingestion-worker

# Restart worker
docker-compose restart ingestion-worker
```

### Out of Memory

```powershell
# Increase Docker Desktop memory limit
# Docker Desktop → Settings → Resources → Memory: 8 GB

# Or reduce services
docker-compose up -d postgres neo4j redis api-gateway graph-engine
```

## 🎯 Next Steps

1. **Test Locally First**
   - Run `.\quick-start.ps1`
   - Verify all health checks pass
   - Test API endpoints with curl/Postman
   - View logs to ensure no errors

2. **Deploy to Railway**
   - Follow [RAILWAY_DEPLOYMENT.md](RAILWAY_DEPLOYMENT.md)
   - Start with databases (PostgreSQL, Neo4j, Redis)
   - Deploy services one by one
   - Configure environment variables from `.env.railway.example`

3. **Set Up Monitoring** (Production)
   - Railway dashboard for metrics
   - Configure log aggregation
   - Set up uptime monitoring (UptimeRobot, Pingdom)

4. **Update VS Code Extension**
   - Update API URLs in `frontend/src/api/client.ts`
   - Point to Railway URLs instead of localhost
   - Rebuild extension: `cd frontend && npm run compile`

## 📚 Additional Resources

- [Docker Compose Documentation](https://docs.docker.com/compose/)
- [Railway Documentation](https://docs.railway.app/)
- [RAILWAY_DEPLOYMENT.md](RAILWAY_DEPLOYMENT.md) - Step-by-step Railway guide
- [INTEGRATIONS_SETUP.md](INTEGRATIONS_SETUP.md) - LLM and webhook setup

## 💡 Tips

- **Use `quick-start.ps1` for the easiest setup experience**
- **Always check logs** with `docker-compose logs -f` when debugging
- **Stop unused services** to save resources: `docker-compose stop ingestion-worker`
- **Backup Neo4j data** regularly: Neo4j exports, or volume snapshots
- **Use Railway's free tier** for initial testing before upgrading
