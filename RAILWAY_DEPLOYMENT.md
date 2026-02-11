# Railway Deployment Guide for ArchMind Backend

## Prerequisites

1. Railway account: https://railway.app
2. Railway CLI installed: `npm install -g @railway/cli`
3. Docker installed locally for testing

## Services to Deploy on Railway

ArchMind requires 6 services on Railway:

1. **PostgreSQL** (Railway Template)
2. **Neo4j** (Docker)
3. **Redis** (Railway Template)
4. **API Gateway** (Docker - Go)
5. **Graph Engine** (Docker - Python)
6. **Ingestion Worker** (Docker - Rust)

## Step 1: Create Railway Project

```bash
railway login
railway init
```

## Step 2: Add Database Services

### PostgreSQL
```bash
railway add --database postgres
```

Get the connection string:
```bash
railway variables
# Copy POSTGRES_URL or DATABASE_URL
```

### Redis
```bash
railway add --database redis
```

Get the connection string:
```bash
railway variables
# Copy REDIS_URL
```

### Neo4j
Create a new service with Docker:
```bash
railway up -d
```

Use this Dockerfile in a separate Neo4j service:
```dockerfile
FROM neo4j:5.16.0
ENV NEO4J_AUTH=neo4j/your-strong-password
```

Or use Neo4j Aura (cloud): https://neo4j.com/cloud/aura/

## Step 3: Deploy API Gateway

```bash
cd apps/api-gateway
railway up
```

Environment variables to set in Railway dashboard:
```
PORT=8080
POSTGRES_URL=${{Postgres.DATABASE_URL}}
NEO4J_URI=bolt://your-neo4j-url:7687
NEO4J_USER=neo4j
NEO4J_PASSWORD=your-neo4j-password
REDIS_URL=${{Redis.REDIS_URL}}
GRAPH_ENGINE_URL=https://your-graph-engine.railway.app
```

## Step 4: Deploy Graph Engine

```bash
cd services/graph-engine
railway up
```

Environment variables:
```
NEO4J_URI=bolt://your-neo4j-url:7687
NEO4J_USER=neo4j
NEO4J_PASSWORD=your-neo4j-password
POSTGRES_URL=${{Postgres.DATABASE_URL}}
REDIS_URL=${{Redis.REDIS_URL}}
HOST=0.0.0.0
PORT=8000
CORS_ORIGINS=*
LOG_LEVEL=INFO
LLM_PROVIDER=gemini
GEMINI_API_KEY=your-gemini-api-key
LLM_MODEL=gemini-3-flash-preview
```

## Step 5: Deploy Ingestion Worker

```bash
cd services/ingestion-worker
railway up
```

Environment variables:
```
DATABASE_URL=${{Postgres.DATABASE_URL}}
NEO4J_URI=bolt://your-neo4j-url:7687
NEO4J_USER=neo4j
NEO4J_PASSWORD=your-neo4j-password
REDIS_URL=${{Redis.REDIS_URL}}
RUST_LOG=info
```

## Step 6: Configure Custom Domains (Optional)

In Railway dashboard:
1. Go to each service > Settings > Networking
2. Add custom domain or use Railway-provided domain
3. Enable HTTPS

## Step 7: Run Database Migrations

Connect to your PostgreSQL instance and run:
```bash
psql $POSTGRES_URL < infra/postgres/init/001_schema.sql
psql $POSTGRES_URL < infra/postgres/init/002_file_contributions.sql
psql $POSTGRES_URL < infra/postgres/init/003_architecture_insights.sql
```

Or use Railway CLI:
```bash
railway run psql $DATABASE_URL < infra/postgres/init/001_schema.sql
```

## Environment Variables Reference

### Required for API Gateway
- `PORT` - Server port (8080)
- `POSTGRES_URL` - PostgreSQL connection string
- `NEO4J_URI` - Neo4j Bolt URI
- `NEO4J_USER` - Neo4j username
- `NEO4J_PASSWORD` - Neo4j password
- `REDIS_URL` - Redis connection string
- `GRAPH_ENGINE_URL` - Graph Engine service URL

### Required for Graph Engine
- `NEO4J_URI` - Neo4j Bolt URI
- `NEO4J_USER` - Neo4j username
- `NEO4J_PASSWORD` - Neo4j password
- `POSTGRES_URL` - PostgreSQL connection string
- `REDIS_URL` - Redis connection string
- `LLM_PROVIDER` - LLM provider (gemini, openai, anthropic, bedrock, ollama)
- `GEMINI_API_KEY` - Google Gemini API key
- `LLM_MODEL` - Model name

### Required for Ingestion Worker
- `DATABASE_URL` - PostgreSQL connection string
- `NEO4J_URI` - Neo4j Bolt URI
- `NEO4J_USER` - Neo4j username
- `NEO4J_PASSWORD` - Neo4j password
- `REDIS_URL` - Redis connection string
- `API_GATEWAY_URL` - Public API Gateway URL (e.g., https://your-api-gateway.up.railway.app)

## Testing Locally with Docker Compose

Before deploying to Railway, test locally:

```bash
# Set environment variables
export POSTGRES_PASSWORD=your-password
export GEMINI_API_KEY=your-gemini-key
export LLM_PROVIDER=gemini
export LLM_MODEL=gemini-3-flash-preview

# Start all services
docker-compose up --build

# Check health
curl http://localhost:8080/health
curl http://localhost:8000/health
```

## Updating Services

To update a service:
```bash
cd services/graph-engine  # or api-gateway, ingestion-worker
railway up
```

## Monitoring

View logs in Railway dashboard or via CLI:
```bash
railway logs
railway logs --service api-gateway
railway logs --service graph-engine
railway logs --service ingestion-worker
```

## Cost Optimization

Railway pricing is based on:
- Resource usage (CPU, Memory, Network)
- Active time

To optimize costs:
1. Use Railway's free tier for databases if possible
2. Scale down worker replicas when not needed
3. Use sleep mode for development environments
4. Monitor resource usage in Railway dashboard

## Troubleshooting

### Service won't start
- Check environment variables are set correctly
- View logs: `railway logs --service <service-name>`
- Verify all dependent services are running

### Database connection errors
- Ensure PostgreSQL/Neo4j/Redis services are healthy
- Check connection URLs have correct credentials
- Verify network connectivity between services

### Build failures
- Check Dockerfile syntax
- Ensure all dependencies are listed in requirements.txt/Cargo.toml/go.mod
- Review build logs in Railway dashboard

## Support

- Railway Docs: https://docs.railway.app
- Railway Discord: https://discord.gg/railway
- Project Issues: https://github.com/your-repo/issues
