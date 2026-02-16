//! Module Boundary Detector
//!
//! Detects and classifies module boundaries in a codebase:
//! - Physical Boundaries: Monorepo workspaces, multi-repo structure
//! - Logical Boundaries: Package/namespace groupings, directory structure
//! - Architectural Boundaries: Presentation, Business Logic, Data Access layers

use crate::parsers::ParsedFile;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, debug};

/// Type of boundary detected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BoundaryType {
    Physical,      // Separate repos or workspace members
    Logical,       // Package/namespace/directory groupings
    Architectural, // Layer-based (Presentation, Business Logic, Data Access)
}

impl BoundaryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BoundaryType::Physical => "physical",
            BoundaryType::Logical => "logical",
            BoundaryType::Architectural => "architectural",
        }
    }
}

/// Architectural layer classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ArchitecturalLayer {
    Presentation,   // UI components, views, controllers
    BusinessLogic,  // Core domain logic, services
    DataAccess,     // Database, ORM, repositories
    Infrastructure, // Config, utilities, middleware
    Unknown,
}

impl ArchitecturalLayer {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArchitecturalLayer::Presentation => "presentation",
            ArchitecturalLayer::BusinessLogic => "business_logic",
            ArchitecturalLayer::DataAccess => "data_access",
            ArchitecturalLayer::Infrastructure => "infrastructure",
            ArchitecturalLayer::Unknown => "unknown",
        }
    }
}

/// Detected boundary in the codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Boundary {
    pub id: String,
    pub name: String,
    pub boundary_type: BoundaryType,
    pub path: String,
    pub layer: Option<ArchitecturalLayer>,
    pub file_count: usize,
    pub files: Vec<String>,
}

/// Module boundary detection results
#[derive(Debug, Clone)]
pub struct BoundaryDetectionResult {
    pub boundaries: Vec<Boundary>,
    pub file_to_boundary: HashMap<String, String>, // file_path -> boundary_id
}

/// Detects module boundaries in a codebase
pub struct BoundaryDetector;

impl BoundaryDetector {
    /// Detect all boundaries in the given files
    pub fn detect_boundaries(
        parsed_files: &[ParsedFile],
        repo_path: &Path,
    ) -> Result<BoundaryDetectionResult> {
        info!("🔍 Detecting module boundaries...");

        let mut boundaries = Vec::new();
        let mut file_to_boundary = HashMap::new();

        // 1. Detect physical boundaries (workspaces)
        let physical = Self::detect_physical_boundaries(repo_path)?;
        boundaries.extend(physical);

        // 2. Detect logical boundaries (directory structure)
        let logical = Self::detect_logical_boundaries(parsed_files)?;
        boundaries.extend(logical);

        // 3. Detect architectural boundaries (layers)
        let architectural = Self::detect_architectural_boundaries(parsed_files)?;
        boundaries.extend(architectural);

        // Build file-to-boundary mapping
        for boundary in &boundaries {
            for file in &boundary.files {
                file_to_boundary.insert(file.clone(), boundary.id.clone());
            }
        }

        info!("✅ Detected {} boundaries", boundaries.len());

        Ok(BoundaryDetectionResult {
            boundaries,
            file_to_boundary,
        })
    }

    /// Detect physical boundaries (monorepo workspaces, multi-repo)
    fn detect_physical_boundaries(repo_path: &Path) -> Result<Vec<Boundary>> {
        let mut boundaries = Vec::new();

        // Check for package.json workspaces
        if let Ok(package_json_path) = repo_path.join("package.json").canonicalize() {
            if package_json_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&package_json_path) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(workspaces) = json.get("workspaces").and_then(|w| w.as_array()) {
                            for (idx, workspace) in workspaces.iter().enumerate() {
                                if let Some(workspace_path) = workspace.as_str() {
                                    boundaries.push(Boundary {
                                        id: format!("physical_workspace_{}", idx),
                                        name: format!("Workspace: {}", workspace_path),
                                        boundary_type: BoundaryType::Physical,
                                        path: workspace_path.to_string(),
                                        layer: None,
                                        file_count: 0,
                                        files: Vec::new(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check for Cargo.toml workspace
        if let Ok(cargo_toml_path) = repo_path.join("Cargo.toml").canonicalize() {
            if cargo_toml_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&cargo_toml_path) {
                    if content.contains("[workspace]") {
                        boundaries.push(Boundary {
                            id: "physical_cargo_workspace".to_string(),
                            name: "Cargo Workspace".to_string(),
                            boundary_type: BoundaryType::Physical,
                            path: ".".to_string(),
                            layer: None,
                            file_count: 0,
                            files: Vec::new(),
                        });
                    }
                }
            }
        }

        // Check for go.work
        if let Ok(go_work_path) = repo_path.join("go.work").canonicalize() {
            if go_work_path.exists() {
                boundaries.push(Boundary {
                    id: "physical_go_workspace".to_string(),
                    name: "Go Workspace".to_string(),
                    boundary_type: BoundaryType::Physical,
                    path: ".".to_string(),
                    layer: None,
                    file_count: 0,
                    files: Vec::new(),
                });
            }
        }

        debug!("Detected {} physical boundaries", boundaries.len());
        Ok(boundaries)
    }

    /// Detect logical boundaries (directory-based modules)
    fn detect_logical_boundaries(parsed_files: &[ParsedFile]) -> Result<Vec<Boundary>> {
        let mut boundaries = Vec::new();
        let mut dir_files: HashMap<String, Vec<String>> = HashMap::new();

        // Group files by their top-level directory
        for file in parsed_files {
            let path_str = &file.path;
            if let Some(top_dir) = Self::extract_top_level_dir(path_str) {
                dir_files.entry(top_dir.to_string())
                    .or_insert_with(Vec::new)
                    .push(path_str.clone());
            }
        }

        // Create boundaries for directories with multiple files
        for (dir, files) in dir_files {
            if files.len() >= 2 {
                // Only create boundary if it has at least 2 files
                boundaries.push(Boundary {
                    id: format!("logical_{}", dir.replace('/', "_").replace('\\', "_")),
                    name: dir.clone(),
                    boundary_type: BoundaryType::Logical,
                    path: dir,
                    layer: None,
                    file_count: files.len(),
                    files,
                });
            }
        }

        debug!("Detected {} logical boundaries", boundaries.len());
        Ok(boundaries)
    }

    /// Detect architectural boundaries (layers)
    fn detect_architectural_boundaries(parsed_files: &[ParsedFile]) -> Result<Vec<Boundary>> {
        let mut layer_files: HashMap<ArchitecturalLayer, Vec<String>> = HashMap::new();

        // Classify each file into an architectural layer
        for file in parsed_files {
            let layer = Self::classify_architectural_layer(&file.path);
            layer_files.entry(layer)
                .or_insert_with(Vec::new)
                .push(file.path.clone());
        }

        // Create boundaries for each layer
        let boundaries: Vec<Boundary> = layer_files
            .into_iter()
            .filter(|(layer, files)| *layer != ArchitecturalLayer::Unknown && files.len() >= 2)
            .map(|(layer, files)| Boundary {
                id: format!("architectural_{}", layer.as_str()),
                name: format!("{:?} Layer", layer),
                boundary_type: BoundaryType::Architectural,
                path: layer.as_str().to_string(),
                layer: Some(layer),
                file_count: files.len(),
                files,
            })
            .collect();

        debug!("Detected {} architectural boundaries", boundaries.len());
        Ok(boundaries)
    }

    /// Extract the top-level directory from a file path
    fn extract_top_level_dir(path: &str) -> Option<&str> {
        let path = path.trim_start_matches("./").trim_start_matches(".\\");
        
        // Find first directory separator
        if let Some(idx) = path.find('/').or_else(|| path.find('\\')) {
            Some(&path[..idx])
        } else {
            None
        }
    }

    /// Classify a file into an architectural layer
    fn classify_architectural_layer(path: &str) -> ArchitecturalLayer {
        let path_lower = path.to_lowercase();

        // Presentation layer indicators
        if path_lower.contains("component")
            || path_lower.contains("view")
            || path_lower.contains("page")
            || path_lower.contains("ui")
            || path_lower.contains("controller")
            || path_lower.contains("route")
            || path_lower.ends_with(".tsx")
            || path_lower.ends_with(".jsx")
        {
            return ArchitecturalLayer::Presentation;
        }

        // Data Access layer indicators
        if path_lower.contains("repository")
            || path_lower.contains("dao")
            || path_lower.contains("model")
            || path_lower.contains("schema")
            || path_lower.contains("database")
            || path_lower.contains("db")
            || path_lower.contains("migration")
        {
            return ArchitecturalLayer::DataAccess;
        }

        // Infrastructure layer indicators
        if path_lower.contains("config")
            || path_lower.contains("util")
            || path_lower.contains("helper")
            || path_lower.contains("middleware")
            || path_lower.contains("plugin")
            || path_lower.contains("infrastructure")
        {
            return ArchitecturalLayer::Infrastructure;
        }

        // Business Logic layer (default for service files)
        if path_lower.contains("service")
            || path_lower.contains("business")
            || path_lower.contains("domain")
            || path_lower.contains("logic")
            || path_lower.contains("usecase")
        {
            return ArchitecturalLayer::BusinessLogic;
        }

        ArchitecturalLayer::Unknown
    }
}
