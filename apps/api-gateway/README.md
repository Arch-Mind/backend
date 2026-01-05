# API Gateway (Go)

The API Gateway is the main entry point for the ArchMind platform. It handles authentication, orchestrates analysis jobs, and manages webhooks.

## Technology Stack

- **Language**: Go 1.21+
- **Framework**: Gin (HTTP router)
- **Database**: PostgreSQL (via lib/pq)
- **Message Queue**: Redis (via go-redis)

## Features

- ✅ RESTful API endpoints
- ✅ Job queue management via Redis
- ✅ PostgreSQL metadata storage
- ✅ CORS support for web dashboard
- ✅ Health check endpoint
- 🚧 GitHub OAuth authentication (planned)
- 🚧 Webhook handling (planned)
- 🚧 Rate limiting (planned)

## Getting Started

### Prerequisites

- Go 1.21 or higher
- Running PostgreSQL instance (via docker-compose)
- Running Redis instance (via docker-compose)

### Installation

```bash
# Install dependencies
go mod download

# Copy environment configuration
cp .env.example .env

# Edit .env with your configuration
```

### Running Locally

```bash
# Development mode
go run main.go

# Build and run
go build -o api-gateway
./api-gateway
```

The server will start on `http://localhost:8080`

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
    "redis": "healthy",
    "postgres": "healthy"
  },
  "timestamp": "2026-01-05T12:00:00Z"
}
```

### Submit Repository for Analysis
```bash
POST /api/v1/analyze
Content-Type: application/json

{
  "repo_url": "https://github.com/username/repo",
  "branch": "main",
  "options": {
    "languages": "rust,go,python"
  }
}
```

**Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "QUEUED",
  "message": "Analysis job created successfully",
  "created_at": "2026-01-05T12:00:00Z"
}
```

### Get Job Status
```bash
GET /api/v1/jobs/:id
```

**Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "repo_url": "https://github.com/username/repo",
  "branch": "main",
  "status": "PROCESSING",
  "options": {
    "languages": "rust,go,python"
  },
  "created_at": "2026-01-05T12:00:00Z"
}
```

### List All Jobs
```bash
GET /api/v1/jobs
```

### List Repositories
```bash
GET /api/v1/repositories
```

### Get Repository by ID
```bash
GET /api/v1/repositories/:id
```

## Architecture

```
┌─────────────┐
│  Client     │
│  Request    │
└──────┬──────┘
       │
       ▼
┌─────────────────┐
│   Gin Router    │
│   (main.go)     │
└────┬────────────┘
     │
     ├─► Health Check
     │
     ├─► POST /analyze ──► Store in PostgreSQL ──► Push to Redis Queue
     │
     ├─► GET /jobs/:id ──► Query PostgreSQL
     │
     └─► GET /repositories ──► Query PostgreSQL
```

## Job Flow

1. Client sends POST request to `/api/v1/analyze`
2. API Gateway validates request
3. Creates unique Job ID (UUID)
4. Stores job metadata in PostgreSQL
5. Pushes job to Redis queue (`analysis_queue`)
6. Rust Ingestion Worker picks up job from queue
7. Client polls `/api/v1/jobs/:id` for status updates

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `PORT` | Server port | `8080` |
| `REDIS_URL` | Redis connection string | `localhost:6379` |
| `POSTGRES_URL` | PostgreSQL connection string | See `.env.example` |
| `GITHUB_CLIENT_ID` | GitHub OAuth client ID | - |
| `GITHUB_CLIENT_SECRET` | GitHub OAuth secret | - |

## Testing

```bash
# Run tests
go test ./...

# Run with coverage
go test -cover ./...

# Test the API
curl http://localhost:8080/health

# Submit analysis job
curl -X POST http://localhost:8080/api/v1/analyze \
  -H "Content-Type: application/json" \
  -d '{"repo_url": "https://github.com/rust-lang/rust", "branch": "master"}'
```

## Building for Production

```bash
# Build binary
go build -o api-gateway -ldflags="-s -w"

# Build Docker image
docker build -t archmind/api-gateway:latest .
```

## Future Enhancements

- [ ] GitHub OAuth integration
- [ ] Webhook support for GitHub/GitLab
- [ ] Rate limiting per user
- [ ] API key authentication
- [ ] GraphQL endpoint
- [ ] WebSocket for real-time updates
- [ ] Metrics and monitoring (Prometheus)
- [ ] Distributed tracing (OpenTelemetry)
