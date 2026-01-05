# Database Initialization Scripts

This directory contains initialization scripts for PostgreSQL and Neo4j databases.

## PostgreSQL

### Automatic Initialization

When using Docker Compose, PostgreSQL will automatically run all `.sql` files in `postgres/init/` during first startup.

### Manual Migration

```bash
# From infra directory
cd postgres
./run-migrations.sh
```

### Direct Execution

```bash
psql -U postgres -d archmind < init/001_schema.sql
```

## Neo4j

### Using Neo4j Browser

1. Open http://localhost:7474
2. Login with `neo4j/password`
3. Copy and paste the contents of `neo4j/init/001_schema.cypher`
4. Execute the script

### Using cypher-shell

```bash
cat neo4j/init/001_schema.cypher | docker exec -i archmind-neo4j cypher-shell -u neo4j -p password
```

### Using API

```bash
curl -X POST http://localhost:7474/db/neo4j/tx/commit \
  -H "Content-Type: application/json" \
  -H "Authorization: Basic $(echo -n 'neo4j:password' | base64)" \
  -d @neo4j/init/001_schema.cypher
```

## Schema Overview

### PostgreSQL Tables

- `users` - User accounts
- `repositories` - Tracked repositories
- `analysis_jobs` - Job queue and status
- `analysis_results` - Analysis metrics
- `api_keys` - API authentication (future)
- `webhooks` - Webhook configurations (future)

### Neo4j Graph

**Nodes:**
- `Repository` - Repository metadata
- `File` - Source files
- `Function` - Functions/methods
- `Class` - Classes/structs
- `Module` - Packages/modules
- `Job` - Analysis jobs

**Relationships:**
- `CONTAINS` - Containment
- `CALLS` - Function invocation
- `IMPORTS` - Import dependency
- `INHERITS` - Class inheritance
- `IMPLEMENTS` - Interface implementation

## Verification

### PostgreSQL

```sql
-- Check tables
\dt

-- Check sample data
SELECT * FROM users;
SELECT * FROM analysis_jobs LIMIT 10;
```

### Neo4j

```cypher
// Count nodes
MATCH (n) RETURN labels(n)[0] AS Type, count(n) AS Count;

// Count relationships
MATCH ()-[r]->() RETURN type(r) AS Type, count(r) AS Count;

// Show sample data
MATCH (r:Repository)-[:CONTAINS]->(f:File)
RETURN r.name, f.path LIMIT 10;
```

## Backup & Restore

### PostgreSQL Backup

```bash
pg_dump -U postgres archmind > backup.sql
```

### PostgreSQL Restore

```bash
psql -U postgres archmind < backup.sql
```

### Neo4j Backup

```bash
docker exec archmind-neo4j neo4j-admin dump --database=neo4j --to=/backups/neo4j.dump
```

### Neo4j Restore

```bash
docker exec archmind-neo4j neo4j-admin load --database=neo4j --from=/backups/neo4j.dump --force
```

## Migration Management

For production, consider using migration tools:
- PostgreSQL: [golang-migrate](https://github.com/golang-migrate/migrate), [Flyway](https://flywaydb.org/)
- Neo4j: [Liquigraph](http://www.liquigraph.org/), custom scripts

## Troubleshooting

### PostgreSQL connection refused
- Check if container is running: `docker ps`
- Check logs: `docker logs archmind-postgres`
- Verify port: `netstat -an | grep 5432`

### Neo4j authentication failed
- Default credentials: `neo4j/password`
- Reset password: `docker exec archmind-neo4j neo4j-admin set-initial-password newpassword`

### Migrations not running
- Check file permissions: `chmod +x run-migrations.sh`
- Verify SQL syntax
- Check Docker volume mounts in docker-compose.yml
