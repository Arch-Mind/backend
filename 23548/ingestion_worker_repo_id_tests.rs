// ===========================================================================
// Ingestion Worker – Repo ID Verification Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parsers::{FunctionInfo, ClassInfo};

    #[test]
    fn test_file_node_includes_repo_id() {
        let map = file_node_to_map("src/main.rs", "rust", "job-123", "repo-456");

        assert_eq!(map.get("repo_id"), Some(&"repo-456".to_string()));
        assert_eq!(map.get("job_id"), Some(&"job-123".to_string()));
        assert_eq!(map.get("path"), Some(&"src/main.rs".to_string()));
        assert_eq!(map.get("id"), Some(&"src/main.rs".to_string()));
    }

    #[test]
    fn test_module_node_includes_repo_id() {
        let map = module_node_to_map("my_module", "job-123", "repo-456");

        assert_eq!(map.get("repo_id"), Some(&"repo-456".to_string()));
        assert_eq!(map.get("job_id"), Some(&"job-123".to_string()));
        assert_eq!(map.get("name"), Some(&"my_module".to_string()));
    }

    #[test]
    fn test_function_node_keys_include_repo_id() {
        let func = FunctionInfo {
            name: "my_func".to_string(),
            params: vec!["arg1".to_string()],
            return_type: Some("void".to_string()),
            calls: vec![],
            start_line: 10,
            end_line: 20,
        };

        let map = function_node_to_map(&func, "src/main.rs", "job-123", "repo-456");

        assert!(map.contains_key("repo_id"));
        assert!(map.contains_key("job_id"));
        assert!(map.contains_key("id"));
        assert!(map.contains_key("name"));
    }

    #[test]
    fn test_class_node_keys_include_repo_id() {
        let map = class_node_to_map("MyClass", "src/main.rs", 10, 20, "job-123", "repo-456");

        assert!(map.contains_key("repo_id"));
        assert!(map.contains_key("job_id"));
        assert!(map.contains_key("id"));
    }

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

        let job: AnalysisJob = serde_json::from_str(json).expect("deserialize");

        assert_eq!(job.job_id, "job-123");
        assert_eq!(job.repo_id, "repo-456");
        assert_eq!(job.repo_url, "https://github.com/test");
    }
}
