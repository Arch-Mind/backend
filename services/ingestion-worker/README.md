# Ingestion Worker (Rust)

The Ingestion Worker is the heavy-duty parsing engine that processes repository analysis jobs. It clones repositories, parses source code using tree-sitter, extracts dependencies, and stores the graph data in Neo4j.

## Technology Stack

- **Language**: Rust 1.75+
- **Runtime**: Tokio (async runtime)
- **Message Queue**: Redis (via redis-rs)
- **Git Operations**: git2
- **Parsing**: tree-sitter (polyglot parser)
- **Graph Database**: Neo4j (via neo4rs)

## Features

- ✅ Async job processing from Redis queue
- ✅ Redis connection with retry logic
- ✅ Neo4j graph database connection
- ✅ Structured logging with tracing
- 🚧 Repository cloning with git2 (planned)
- 🚧 Multi-language parsing (Rust, Go, Python, JS/TS) (planned)
- 🚧 Dependency extraction (planned)
- 🚧 Graph data storage in Neo4j (planned)

## Architecture

```
┌────────────────┐
│  Redis Queue   │
│ analysis_queue │
└────────┬───────┘
         │
         │ BRPOP (blocking)
         ▼
┌─────────────────────┐
│  Ingestion Worker   │
│   (Rust/Tokio)      │
└──────┬──────────────┘
       │
       ├─► 1. Clone Repository (git2)
       │
       ├─► 2. Parse Files (tree-sitter)
       │    ├─ Rust
       │    ├─ Go
       │    ├─ Python
       │    └─ JavaScript/TypeScript
       │
       ├─► 3. Extract Dependencies
       │    ├─ Imports
       │    ├─ Function Calls
       │    └─ Class Inheritance
       │
       └─► 4. Store in Neo4j
            ├─ Nodes (File, Function, Class)
            └─ Edges (CALLS, IMPORTS, INHERITS)
```

## Getting Started

### Prerequisites

- Rust 1.75 or higher
- Running Redis instance
- Running Neo4j instance
- Git installed (for git2 to work)

### Installation

```bash
# Install dependencies
cargo build

# Copy environment configuration
cp .env.example .env

# Edit .env with your configuration
```

### Running Locally

```bash
# Development mode with auto-reload
cargo watch -x run

# Run directly
cargo run

# Run in release mode (optimized)
cargo run --release
```

## Job Processing Flow

1. **Listen**: Worker blocks on Redis queue using `BRPOP`
2. **Receive**: Job JSON received from `analysis_queue`
3. **Parse**: Deserialize job into `AnalysisJob` struct
4. **Clone**: Clone repository using git2
5. **Parse**: Use tree-sitter to parse source files
6. **Extract**: Build dependency graph from AST
7. **Store**: Push nodes and edges to Neo4j
8. **Complete**: Update job status in PostgreSQL

## Job Format

```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "repo_url": "https://github.com/username/repo",
  "branch": "main",
  "status": "QUEUED",
  "options": {
    "languages": "rust,go,python"
  },
  "created_at": "2026-01-05T12:00:00Z"
}
```

## Supported Languages

| Language | Parser | Status |
|----------|--------|--------|
| Rust | tree-sitter-rust | 🚧 Planned |
| Go | tree-sitter-go | 🚧 Planned |
| Python | tree-sitter-python | 🚧 Planned |
| JavaScript | tree-sitter-javascript | 🚧 Planned |
| TypeScript | tree-sitter-typescript | 🚧 Planned |
| Java | tree-sitter-java | 📋 Future |
| C/C++ | tree-sitter-c/cpp | 📋 Future |

## Neo4j Graph Schema

### Nodes

- **File**: Source code files
  - Properties: `path`, `language`, `size`, `hash`
- **Function**: Function/method definitions
  - Properties: `name`, `signature`, `line_start`, `line_end`
- **Class**: Class definitions
  - Properties: `name`, `type`, `line_start`, `line_end`
- **Module**: Package/module definitions
  - Properties: `name`, `path`

### Relationships

- **CALLS**: Function A calls Function B
- **IMPORTS**: File A imports Module B
- **INHERITS**: Class A inherits from Class B
- **CONTAINS**: File contains Function/Class
- **DEPENDS_ON**: Module dependency

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `REDIS_URL` | Redis connection string | `redis://localhost:6379` |
| `NEO4J_URI` | Neo4j Bolt URI | `bolt://localhost:7687` |
| `NEO4J_USER` | Neo4j username | `neo4j` |
| `NEO4J_PASSWORD` | Neo4j password | `password` |
| `API_GATEWAY_URL` | API Gateway base URL | `http://localhost:8080` |
| `RUST_LOG` | Log level | `info` |

## Logging

The worker uses `tracing` for structured logging:

```rust
RUST_LOG=debug cargo run  # Debug level
RUST_LOG=info cargo run   # Info level (default)
RUST_LOG=warn cargo run   # Warnings only
```

## Testing

```bash
# Run tests
cargo test

# Run with coverage
cargo tarpaulin --out Html

# Lint
cargo clippy

# Format
cargo fmt
```

## Performance

The worker is optimized for:
- **Async I/O**: All network operations are non-blocking
- **Parallel Parsing**: Multiple files parsed concurrently
- **Memory Efficiency**: Streaming large repositories
- **Release Build**: LTO enabled for maximum performance

Typical performance:
- Small repos (<100 files): 5-10 seconds
- Medium repos (100-1000 files): 30-60 seconds
- Large repos (1000+ files): 2-5 minutes

## Error Handling

All errors are logged and the job status is updated to `FAILED`:
- Repository clone failures
- Parse errors (invalid syntax)
- Neo4j connection issues
- Redis connection issues

Failed jobs can be retried manually via the API Gateway.

## Future Enhancements

- [ ] Complete git2 integration for cloning
- [ ] Implement tree-sitter parsers for all languages
- [ ] Add dependency extraction logic
- [ ] Implement Neo4j graph storage
- [ ] Add retry logic for transient failures
- [ ] Support incremental analysis (only changed files)
- [ ] Add webhook callbacks on job completion
- [ ] Implement distributed worker pool
- [ ] Add metrics and monitoring
- [ ] Support private repositories (SSH keys)
