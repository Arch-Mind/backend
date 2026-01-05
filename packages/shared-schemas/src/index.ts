/**
 * Shared TypeScript Types for ArchMind Platform
 */

// ==================== Analysis Job ====================

export interface AnalysisJob {
  job_id: string;
  repo_url: string;
  branch: string;
  status: JobStatus;
  options?: Record<string, string>;
  created_at: string;
  updated_at?: string;
  completed_at?: string;
  error_message?: string;
}

export type JobStatus = 
  | "QUEUED"
  | "PROCESSING"
  | "COMPLETED"
  | "FAILED"
  | "CANCELLED";

export interface AnalyzeRequest {
  repo_url: string;
  branch?: string;
  options?: Record<string, string>;
}

export interface AnalyzeResponse {
  job_id: string;
  status: JobStatus;
  message: string;
  created_at: string;
}

// ==================== Repository ====================

export interface Repository {
  id: number;
  url: string;
  owner_id: number;
  name: string;
  description?: string;
  created_at: string;
  updated_at: string;
  last_analyzed?: string;
}

// ==================== Graph Nodes & Edges ====================

export type NodeType = 
  | "File"
  | "Function"
  | "Class"
  | "Module"
  | "Interface";

export type RelationshipType =
  | "CALLS"
  | "IMPORTS"
  | "INHERITS"
  | "CONTAINS"
  | "DEPENDS_ON"
  | "IMPLEMENTS";

export interface GraphNode {
  id: string;
  label: string;
  type: NodeType;
  properties: Record<string, any>;
}

export interface GraphEdge {
  source: string;
  target: string;
  type: RelationshipType;
  properties?: Record<string, any>;
}

export interface DependencyGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

// ==================== Metrics ====================

export interface RepositoryMetrics {
  total_files: number;
  total_functions: number;
  total_classes: number;
  total_dependencies: number;
  complexity_score: number;
  lines_of_code?: number;
  languages?: Record<string, number>;
}

export interface PageRankResult {
  repo_id: string;
  total_nodes: number;
  top_nodes: Array<{
    id: string;
    score: number;
    name?: string;
    type?: NodeType;
  }>;
}

export interface ImpactAnalysisResult {
  node_id: string;
  impacted_count: number;
  impacted_nodes: Array<{
    id: string;
    name: string;
    type: string;
    distance: number;
  }>;
}

// ==================== User ====================

export interface User {
  id: number;
  github_id?: string;
  email: string;
  username: string;
  avatar_url?: string;
  created_at: string;
  updated_at: string;
}

// ==================== API Responses ====================

export interface ApiError {
  error: string;
  details?: string;
  code?: string;
}

export interface HealthCheckResponse {
  status: "ok" | "degraded" | "down";
  services: Record<string, "healthy" | "unhealthy" | "disconnected">;
  timestamp?: string;
}

export interface ListResponse<T> {
  items: T[];
  total: number;
  page?: number;
  page_size?: number;
}

// ==================== Parsed Code ====================

export interface ParsedFile {
  path: string;
  language: string;
  functions: string[];
  classes: string[];
  imports: string[];
  exports?: string[];
  line_count?: number;
}

export interface Dependency {
  from: string;
  to: string;
  relationship_type: RelationshipType;
  file_path?: string;
  line_number?: number;
}

// ==================== Configuration ====================

export interface AnalysisOptions {
  languages?: string[];
  include_tests?: boolean;
  max_depth?: number;
  ignore_patterns?: string[];
}

export interface PlatformConfig {
  api_gateway_url: string;
  graph_engine_url: string;
  redis_url: string;
  postgres_url: string;
  neo4j_uri: string;
}
