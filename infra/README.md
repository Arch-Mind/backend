# Infrastructure Configuration

This directory contains the infrastructure configuration for ArchMind local development and deployment.

## Services

### PostgreSQL (Port 5432)
- **Purpose**: Metadata storage for users, repositories, and analysis jobs
- **Credentials**: `postgres/postgres`
- **Database**: `archmind`
- **Connection**: `postgresql://postgres:postgres@localhost:5432/archmind`

### Neo4j (Ports 7474, 7687)
- **Purpose**: Graph database for dependency relationships
- **Credentials**: `neo4j/password`
- **HTTP Browser**: http://localhost:7474
- **Bolt Protocol**: bolt://localhost:7687
- **Plugins**: APOC, Graph Data Science

### Redis (Port 6379)
- **Purpose**: Message broker and job queue
- **Connection**: `redis://localhost:6379`
- **Persistence**: AOF enabled
- **Max Memory**: 512MB with LRU eviction

### MinIO (Ports 9000, 9001)
- **Purpose**: Object storage for large repository artifacts
- **API**: http://localhost:9000
- **Console**: http://localhost:9001
- **Credentials**: `minioadmin/minioadmin`

## Quick Start

```bash
# Start all services
docker-compose up -d

# View logs
docker-compose logs -f

# Stop all services
docker-compose down

# Stop and remove volumes (WARNING: destroys data)
docker-compose down -v

# Restart a specific service
docker-compose restart postgres
```

## Health Checks

All services include health checks. Check status:

```bash
docker-compose ps
```

## Database Initialization

### PostgreSQL

Migrations are automatically applied from `./postgres/init/*.sql` on first startup.

Manual migration:
```bash
docker exec -i archmind-postgres psql -U postgres -d archmind < postgres/migrations/001_initial.sql
```

### Neo4j

Constraints and indexes are created from `./neo4j/init/*.cypher` scripts.

Manual execution:
```bash
docker exec -i archmind-neo4j cypher-shell -u neo4j -p password < neo4j/init/001_schema.cypher
```

## Network

All services are connected via the `codepulse-net` bridge network, allowing inter-service communication using container names.

Example from API Gateway:
```go
// Connect to Redis
redisClient := redis.NewClient(&redis.Options{
    Addr: "redis:6379",
})
```

## Volume Management

Data persists in Docker volumes:
- `postgres_data` - PostgreSQL database
- `neo4j_data` - Neo4j graph database
- `neo4j_logs` - Neo4j logs
- `redis_data` - Redis snapshots
- `minio_data` - MinIO object storage

Backup volumes:
```bash
docker run --rm -v postgres_data:/data -v $(pwd)/backup:/backup alpine tar czf /backup/postgres_backup.tar.gz /data
```

## Environment Variables

Create a `.env` file in this directory to override defaults:

```env
# PostgreSQL
POSTGRES_PASSWORD=your_secure_password

# Neo4j
NEO4J_PASSWORD=your_secure_password

# MinIO
MINIO_ROOT_PASSWORD=your_secure_password
```

## Production Deployment

For production:
1. Use strong passwords
2. Enable SSL/TLS for all services
3. Configure resource limits
4. Set up monitoring and alerting
5. Implement backup strategies
6. Use managed services where possible

See `docker-compose.prod.yml` for production configuration.
