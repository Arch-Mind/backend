
// ============================================================================
// Repo ID Verification Tests
// ============================================================================
// This file consolidates the unit tests implemented to verify repo_id support.

// ----------------------------------------------------------------------------
// 1. Node Mapping Logic Tests (Added to neo4j_storage.rs)
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{FunctionInfo, ClassInfo};

    #[test]
    fn test_file_node_mapping_includes_repo_id() {
        let job_id = "job-123";
        let repo_id = "repo-456";
        let path = "src/main.rs";
        let language = "rust";

        let map = file_node_to_map(path, language, job_id, repo_id);

        assert_eq!(map.get("repo_id"), Some(&repo_id.to_string()));
        assert_eq!(map.get("job_id"), Some(&job_id.to_string()));
        assert_eq!(map.get("path"), Some(&path.to_string()));
        assert_eq!(map.get("id"), Some(&path.to_string()));
    }

    #[test]
    fn test_module_node_mapping_includes_repo_id() {
        let job_id = "job-123";
        let repo_id = "repo-456";
        let name = "my_module";

        let map = module_node_to_map(name, job_id, repo_id);

        assert_eq!(map.get("repo_id"), Some(&repo_id.to_string()));
        assert_eq!(map.get("job_id"), Some(&job_id.to_string()));
        assert_eq!(map.get("name"), Some(&name.to_string()));
    }

    #[test]
    fn test_function_node_keys_include_repo_id() {
        let job_id = "job-123";
        let repo_id = "repo-456";
        let file = "src/main.rs";
        
        let func = FunctionInfo {
            name: "my_func".to_string(),
            params: vec!["arg1".to_string()],
            return_type: Some("void".to_string()),
            calls: vec![],
            start_line: 10,
            end_line: 20,
        };

        let map = function_node_to_map(&func, file, job_id, repo_id);

        assert!(map.contains_key("repo_id"));
        assert!(map.contains_key("job_id"));
        assert!(map.contains_key("id"));
        assert!(map.contains_key("name"));
    }

    #[test]
    fn test_class_node_keys_include_repo_id() {
        let job_id = "job-123";
        let repo_id = "repo-456";
        let file = "src/main.rs";
        let name = "MyClass";

        let map = class_node_to_map(name, file, 10, 20, job_id, repo_id);

        assert!(map.contains_key("repo_id"));
        assert!(map.contains_key("job_id"));
        assert!(map.contains_key("id"));
    }
}

// ----------------------------------------------------------------------------
// 2. Job Parsing Tests (Added to tests.rs)
// ----------------------------------------------------------------------------

#[test]
fn test_analysis_job_deserialization_with_repo_id() {
    let json = r#"{
        "job_id": "job-123",
        "repo_id": "repo-456",
        "repo_url": "https://github.com/test",
        "branch": "main",
        "status": "QUEUED",
        "created_at": "2023-01-01T00:00:00Z"
    }"#;

    // We use super::AnalysisJob in the actual implementation
    let job: AnalysisJob = serde_json::from_str(json).expect("Failed to deserialize");
    
    assert_eq!(job.job_id, "job-123");
    assert_eq!(job.repo_id, "repo-456");
    assert_eq!(job.repo_url, "https://github.com/test");
}
