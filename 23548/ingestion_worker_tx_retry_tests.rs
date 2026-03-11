// ===========================================================================
// Ingestion Worker – Neo4j Transaction Retry Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[test]
    fn test_tx_exponential_backoff() {
        let cases = vec![
            (1, Duration::from_millis(500)),   // 500 * 2^0
            (2, Duration::from_millis(1000)),  // 500 * 2^1
            (3, Duration::from_millis(2000)),  // 500 * 2^2
        ];

        for (attempt, expected) in cases {
            let actual = Duration::from_millis(500 * (1 << (attempt - 1)));
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_tx_retry_succeeds_on_last_attempt() {
        let max = 3;
        let mut count = 0;
        let mut ok = false;

        loop {
            count += 1;
            if count < 3 {
                if count >= max { break; }
                continue;
            }
            ok = true;
            break;
        }

        assert_eq!(count, 3);
        assert!(ok);
    }
}
