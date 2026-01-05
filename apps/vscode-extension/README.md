# VS Code Extension for ArchMind

> In-editor codebase intelligence and architecture visualization

## Features (Planned)

- **Workspace Analysis**: Analyze current workspace and submit to ArchMind
- **Dependency Graph**: View interactive dependency graph within VS Code
- **Impact Analysis**: See what code is affected by changes to the current file
- **Code Navigation**: Jump to dependencies and dependents
- **Real-time Updates**: Live updates as you code
- **Inline Metrics**: Display complexity scores and PageRank

## Development

This extension is currently a placeholder structure. Full implementation coming soon.

### Prerequisites

- Node.js 20+
- VS Code 1.85+

### Setup

```bash
# Install dependencies
npm install

# Compile TypeScript
npm run compile

# Watch for changes
npm run watch
```

### Testing

1. Open this folder in VS Code
2. Press F5 to launch Extension Development Host
3. Run commands from Command Palette (Ctrl+Shift+P):
   - `ArchMind: Analyze Workspace`
   - `ArchMind: Show Dependency Graph`
   - `ArchMind: Impact Analysis`

## Commands

- `archmind.analyzeWorkspace` - Submit workspace for analysis
- `archmind.showDependencyGraph` - Open dependency graph viewer
- `archmind.impactAnalysis` - Show impact of changes to current file

## Configuration

Configure in VS Code settings:

```json
{
  "archmind.apiGatewayUrl": "http://localhost:8080",
  "archmind.graphEngineUrl": "http://localhost:8000",
  "archmind.autoAnalyze": false
}
```

## Publishing

```bash
# Package extension
vsce package

# Publish to marketplace
vsce publish
```

## Future Features

- [ ] Implement extension activation
- [ ] Add webview for graph visualization
- [ ] Implement API client
- [ ] Add status bar items
- [ ] Code lens integration
- [ ] Hover provider for metrics
- [ ] Diagnostics for code smells
- [ ] TreeView for dependencies
