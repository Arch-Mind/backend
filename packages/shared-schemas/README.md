# Shared Schemas Package

Shared TypeScript types and JSON schemas used across the ArchMind platform.

## Usage

### In TypeScript/JavaScript Projects

```typescript
import { AnalysisJob, GraphNode, RepositoryMetrics } from "@archmind/shared-schemas";

const job: AnalysisJob = {
  job_id: "123",
  repo_url: "https://github.com/user/repo",
  branch: "main",
  status: "QUEUED",
  created_at: new Date().toISOString()
};
```

### Building

```bash
npm run build
```

### Watching for Changes

```bash
npm run watch
```

## Types Included

- `AnalysisJob` - Job tracking
- `Repository` - Repository metadata
- `GraphNode` - Dependency graph nodes
- `GraphEdge` - Dependency graph edges
- `RepositoryMetrics` - Analysis metrics
- `PageRankResult` - PageRank calculation results
- `ImpactAnalysisResult` - Impact analysis results
- `User` - User accounts
- And more...

## Integration

### Next.js (web-dashboard)
```json
{
  "dependencies": {
    "@archmind/shared-schemas": "workspace:*"
  }
}
```

### VS Code Extension
```json
{
  "dependencies": {
    "@archmind/shared-schemas": "file:../../packages/shared-schemas"
  }
}
```

## Development

When making changes, rebuild the package:

```bash
npm run build
```

Other projects will pick up the changes automatically.
