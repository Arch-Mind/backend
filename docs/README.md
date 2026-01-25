# ArchMind Documentation

Welcome to the ArchMind documentation! This folder contains detailed guides, implementation details, and reference materials for the platform.

## 📚 Documentation Index

### Getting Started

- **[Getting Started Guide](GETTING_STARTED.md)** - Complete setup instructions for local development

### Implementation Details

- **[Tree-sitter Implementation](TREE_SITTER_IMPLEMENTATION.md)** - Deep dive into JavaScript/TypeScript parsing with Tree-sitter
  - Parser architecture
  - Query patterns
  - Data extraction process
  - Neo4j storage schema
  - Usage examples and test cases

### Development

- **[GitHub Issues](GITHUB_ISSUES.md)** - Active issues, feature requests, and development roadmap

## 🏗️ Architecture Overview

ArchMind is a backend platform with three main components:

1. **API Gateway (Go)** - REST API and job orchestration
2. **Ingestion Worker (Rust)** - Code parsing with Tree-sitter
3. **Graph Engine (Python)** - Graph analytics and queries

## 🔗 Quick Links

### Main Documentation
- [Main README](../README.md) - Project overview and quick start

### Service Documentation
- [API Gateway README](../apps/api-gateway/README.md)
- [Ingestion Worker README](../services/ingestion-worker/README.md)
- [Graph Engine README](../services/graph-engine/README.md)

### Infrastructure
- [Docker Compose Setup](../infra/README.md)
- [PostgreSQL Migrations](../infra/postgres/README.md)
- [Neo4j Setup](../infra/neo4j/README.md)

## 🎯 Key Concepts

### Tree-sitter Parsing

Tree-sitter is an incremental parsing library that generates concrete syntax trees (CSTs) from source code. ArchMind uses it to:

- Parse JavaScript/TypeScript files into Abstract Syntax Trees (ASTs)
- Extract function definitions, calls, and imports
- Build dependency graphs across entire codebases

### Graph Database (Neo4j)

Neo4j stores the parsed code structure as a graph:

- **Nodes**: Files, Functions, Modules, Jobs
- **Relationships**: DEFINES, CALLS, IMPORTS

This enables powerful queries like:
- "What will break if I change function X?"
- "Which functions are most critical?"
- "Are there circular dependencies?"

### Event-Driven Architecture

Jobs flow through the system via Redis message queues:

1. API Gateway pushes jobs to Redis
2. Worker(s) pop jobs and process them
3. Results stored in Neo4j
4. Graph Engine queries for analytics

This design allows horizontal scaling and fault tolerance.

## 🔍 Common Queries

### Finding All Functions in a File

```cypher
MATCH (f:File {path: "src/api/users.js"})-[:DEFINES]->(fn:Function)
RETURN fn.name, fn.start_line, fn.end_line
```

### Impact Analysis

```cypher
MATCH path = (fn:Function {name: "validateUser"})<-[:CALLS*]-(caller)
RETURN DISTINCT caller.name AS impacted, length(path) AS depth
ORDER BY depth
```

### Module Dependencies

```cypher
MATCH (f:File)-[:IMPORTS]->(m:Module)
WHERE f.path STARTS WITH "src/"
RETURN f.path, collect(m.name) AS dependencies
```

## 🤝 Contributing to Documentation

When adding new features or making changes:

1. Update relevant documentation files
2. Add examples and usage patterns
3. Update the main README if architecture changes
4. Keep this index up to date

## 📝 Documentation Standards

- Use Markdown for all documentation
- Include code examples with syntax highlighting
- Add diagrams for complex architecture (ASCII art or Mermaid)
- Keep language clear and concise
- Provide both high-level overviews and detailed examples

## 🚀 Need Help?

- Check the [Getting Started Guide](GETTING_STARTED.md) for setup issues
- Review [Tree-sitter Implementation](TREE_SITTER_IMPLEMENTATION.md) for parser questions
- Open an issue on GitHub for bugs or feature requests
