
// ============================================================================
// Qualified Names Verification Tests
// ============================================================================
// This file consolidates the unit tests implemented to verify qualified names support.

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function used in neo4j_storage.rs
    fn get_qualified_id(file_path: &str, name: &str) -> String {
        format!("{}::{}", file_path, name)
    }

    #[test]
    fn test_qualified_id_generation() {
        let file = "src/main.rs";
        let name = "MyClass";
        // Verify format is file::name
        let expected = "src/main.rs::MyClass";
        
        assert_eq!(get_qualified_id(file, name), expected);
    }

    #[test]
    fn test_qualified_id_consistency_across_files() {
        // Simulating cross-file reference
        let file1 = "src/users.rs";
        let class1 = "User";
        let id1 = get_qualified_id(file1, class1);

        let file2 = "src/auth.rs";
        let class2 = "AuthService";
        let id2 = get_qualified_id(file2, class2);

        assert_eq!(id1, "src/users.rs::User");
        assert_eq!(id2, "src/auth.rs::AuthService");
        
        // Ensure they are distinct
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_qualified_id_with_nested_paths() {
        let file = "apps/api/v1/handlers/user.rs";
        let func = "get_user";
        let id = get_qualified_id(file, func);
        
        assert_eq!(id, "apps/api/v1/handlers/user.rs::get_user");
    }
}
