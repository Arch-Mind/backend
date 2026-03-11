// ===========================================================================
// Ingestion Worker – Neo4j Batch Insert Tests
// ===========================================================================

#[cfg(test)]
mod tests {

    struct MockNode { id: String }

    fn batch_chunks(nodes: &[MockNode], size: usize) -> Vec<usize> {
        nodes.chunks(size).map(|c| c.len()).collect()
    }

    #[test]
    fn test_batch_even_split() {
        let nodes: Vec<MockNode> = (0..100).map(|i| MockNode { id: format!("n-{}", i) }).collect();
        let chunks = batch_chunks(&nodes, 25);

        assert_eq!(chunks.len(), 4);
        assert!(chunks.iter().all(|&s| s == 25));
    }

    #[test]
    fn test_batch_uneven_split() {
        let nodes: Vec<MockNode> = (0..105).map(|i| MockNode { id: format!("n-{}", i) }).collect();
        let chunks = batch_chunks(&nodes, 100);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], 100);
        assert_eq!(chunks[1], 5);
    }

    #[test]
    fn test_batch_under_limit() {
        let nodes: Vec<MockNode> = (0..50).map(|i| MockNode { id: format!("n-{}", i) }).collect();
        let chunks = batch_chunks(&nodes, 100);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], 50);
    }

    #[test]
    fn test_retry_query_macro_logic() {
        let max = 3;

        // All retries fail
        let mut a1 = 0;
        let r1: Result<&str, &str> = loop {
            a1 += 1;
            if a1 >= max { break Err("Failed completely"); }
        };
        assert_eq!(a1, 3);
        assert_eq!(r1, Err("Failed completely"));

        // Succeeds on third attempt
        let mut a2 = 0;
        let r2: Result<&str, &str> = loop {
            a2 += 1;
            if a2 < 3 { continue; }
            break Ok("Success");
        };
        assert_eq!(a2, 3);
        assert_eq!(r2, Ok("Success"));
    }
}
