# Tree-sitter JavaScript/TypeScript Integration

## Overview

The Ingestion Worker now includes full Tree-sitter integration for parsing JavaScript and TypeScript source code. This implementation addresses GitHub Issue #1: "Tree-sitter integration for Javascript".

## What's Implemented

### 1. Language Parsers

**Location**: `services/ingestion-worker/src/parsers/`

- `javascript.rs` - JavaScript/JSX parser using tree-sitter-javascript
- `typescript.rs` - TypeScript/TSX parser using tree-sitter-typescript
- `mod.rs` - Common parser trait and data structures

### 2. Capabilities

✅ **Function Detection**
- Regular function declarations: `function greet() {}`
- Arrow functions: `const add = (a, b) => a + b`
- Function expressions: `const fn = function() {}`
- Method definitions: `class X { method() {} }`

✅ **Call Graph Extraction**
- Identifies function calls within each function
- Supports direct calls: `myFunction()`
- Supports method calls: `obj.method()`
- Tracks call relationships for dependency graph

✅ **Import Analysis**
- ES6 imports: `import { foo } from 'bar'`
- CommonJS requires: `const axios = require('axios')`
- Extracts module dependencies

✅ **Repository Scanning**
- Recursively walks directory tree
- Skips common ignore patterns (node_modules, .git, dist, build)
- Processes .js, .jsx, .ts, .tsx files

✅ **Neo4j Integration**
- Stores parsed files as `File` nodes
- Creates `Function` nodes with line numbers
- Establishes `DEFINES`, `CALLS`, and `IMPORTS` relationships
- Links all data to analysis job

## Data Model

### Neo4j Graph Schema

```cypher
// Nodes
(:Job {id, status, timestamp})
(:File {path, language, job_id})
(:Function {name, file, start_line, end_line, job_id})
(:Module {name})

// Relationships
(File)-[:DEFINES]->(Function)
(Function)-[:CALLS]->(Function)
(File)-[:IMPORTS]->(Module)
```

### Example Graph

```
┌─────────────┐
│ File: app.js│
└──────┬──────┘
       │ DEFINES
       ▼
┌──────────────┐        ┌──────────────┐
│ Function:    │ CALLS  │ Function:    │
│ processData()├───────►│ fetchAPI()   │
└──────────────┘        └──────────────┘
       │
       │ IMPORTS
       ▼
┌──────────────┐
│ Module: axios│
└──────────────┘
```

## Usage Example

### Input JavaScript File

```javascript
// src/api/users.js
import axios from 'axios';
import { validateUser } from './validation';

async function fetchUsers() {
    const response = await axios.get('/api/users');
    return processUsers(response.data);
}

function processUsers(data) {
    return data.filter(user => validateUser(user));
}

export { fetchUsers, processUsers };
```

### Parsed Output

**Functions Detected:**
- `fetchUsers` (lines 4-7)
  - Calls: `get`, `processUsers`
- `processUsers` (lines 9-11)
  - Calls: `filter`, `validateUser`

**Imports Detected:**
- `axios`
- `./validation`

**Neo4j Graph Created:**
```cypher
// File node
CREATE (f:File {path: "src/api/users.js", language: "javascript"})

// Function nodes
CREATE (fn1:Function {name: "fetchUsers", start_line: 4, end_line: 7})
CREATE (fn2:Function {name: "processUsers", start_line: 9, end_line: 11})

// Module nodes
CREATE (m1:Module {name: "axios"})
CREATE (m2:Module {name: "./validation"})

// Relationships
CREATE (f)-[:DEFINES]->(fn1)
CREATE (f)-[:DEFINES]->(fn2)
CREATE (f)-[:IMPORTS]->(m1)
CREATE (f)-[:IMPORTS]->(m2)
CREATE (fn1)-[:CALLS]->(fn2)
CREATE (fn2)-[:CALLS {external: true}]->(m2)
```

## Testing

Run the test suite:

```bash
cd services/ingestion-worker
cargo test
```

**Test Coverage:**
- JavaScript function parsing
- TypeScript function parsing
- Import statement extraction
- Function call detection
- Multi-file dependency resolution

## Next Steps / Future Enhancements

### Potential Improvements

1. **Class Support**
   - Extract class definitions
   - Track class methods
   - Detect inheritance relationships

2. **Enhanced Call Graph**
   - Scope-aware call detection (only calls within function body)
   - Handle dynamic imports
   - Track async/await patterns

3. **Type Information (TypeScript)**
   - Extract type definitions
   - Track interface usage
   - Analyze type dependencies

4. **Cross-file Resolution**
   - Resolve import paths to actual files
   - Build complete call graph across modules
   - Detect circular dependencies

5. **Additional Languages**
   - Extend to Python, Go, Rust (already have tree-sitter deps)
   - Unified parser interface

## Architecture

```
┌────────────────────────────────────────────────────────┐
│                   Ingestion Worker                      │
│                                                          │
│  ┌──────────────┐      ┌─────────────────────────┐    │
│  │ Redis Queue  │─────►│   Main Loop             │    │
│  │  (Jobs)      │      │   - Clone repo          │    │
│  └──────────────┘      │   - Walk directory      │    │
│                        └───────────┬─────────────┘    │
│                                    │                    │
│                        ┌───────────▼─────────────┐    │
│                        │  Language Parsers        │    │
│                        │  - JavaScriptParser      │    │
│                        │  - TypeScriptParser      │    │
│                        └───────────┬─────────────┘    │
│                                    │                    │
│                        ┌───────────▼─────────────┐    │
│                        │  Dependency Extractor    │    │
│                        │  - Build call graph      │    │
│                        │  - Map imports           │    │
│                        └───────────┬─────────────┘    │
│                                    │                    │
│                        ┌───────────▼─────────────┐    │
│                        │  Neo4j Storage           │    │
│                        │  - Create nodes          │    │
│                        │  - Create relationships  │    │
│                        └──────────────────────────┘    │
└────────────────────────────────────────────────────────┘
```

## Performance Considerations

- **Incremental Parsing**: Currently parses entire repository. Could implement incremental updates.
- **Parallelization**: File parsing could be parallelized using Rayon.
- **Memory**: Large repositories might benefit from streaming approach.
- **Caching**: Could cache parsed results keyed by file hash.

## Configuration

The parsers are configured via environment variables in `.env`:

```env
REDIS_URL=redis://localhost:6379
NEO4J_URI=bolt://localhost:7687
NEO4J_USER=neo4j
NEO4J_PASSWORD=password
```

## Troubleshooting

### Common Issues

**Issue**: Parser fails on specific syntax
- **Solution**: Ensure tree-sitter grammar is up to date. Check tree-sitter-javascript and tree-sitter-typescript versions.

**Issue**: Functions not being detected
- **Solution**: Check the Tree-sitter query patterns in the parser. Some exotic syntax might need additional query patterns.

**Issue**: Imports not resolved correctly
- **Solution**: Import resolution is currently basic (string extraction). Full path resolution requires additional logic.

## References

- [Tree-sitter Documentation](https://tree-sitter.github.io/tree-sitter/)
- [tree-sitter-javascript Grammar](https://github.com/tree-sitter/tree-sitter-javascript)
- [tree-sitter-typescript Grammar](https://github.com/tree-sitter/tree-sitter-typescript)
- [Neo4j Cypher Manual](https://neo4j.com/docs/cypher-manual/)
