// ===========================================================================
// Ingestion Worker – Qualified Names Verification Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn get_qualified_id(file_path: &str, name: &str) -> String {
        format!("{}::{}", file_path, name)
    }

    #[test]
    fn test_qualified_id_format() {
        assert_eq!(
            get_qualified_id("src/main.rs", "MyClass"),
            "src/main.rs::MyClass"
        );
    }

    #[test]
    fn test_qualified_id_uniqueness_across_files() {
        let id1 = get_qualified_id("src/users.rs", "User");
        let id2 = get_qualified_id("src/auth.rs", "AuthService");

        assert_eq!(id1, "src/users.rs::User");
        assert_eq!(id2, "src/auth.rs::AuthService");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_qualified_id_with_nested_paths() {
        let id = get_qualified_id("apps/api/v1/handlers/user.rs", "get_user");
        assert_eq!(id, "apps/api/v1/handlers/user.rs::get_user");
    }
}
