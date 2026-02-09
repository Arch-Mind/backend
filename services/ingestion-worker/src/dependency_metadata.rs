use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDependency {
    pub name: String,
    pub version: Option<String>,
    pub source_file: String,
}
