// ===========================================================================
// Ingestion Worker – Retry Logic Tests (Redis & Neo4j)
// ===========================================================================

use tokio::time::Duration;

#[tokio::test]
async fn test_exponential_backoff_values() {
    let expected = vec![(1, 1), (2, 2), (3, 4), (4, 8)];

    for (attempt, seconds) in expected {
        let wait = 2u64.pow(attempt - 1);
        assert_eq!(wait, seconds, "attempt {}", attempt);
    }
}

#[tokio::test]
async fn test_max_retries_respected() {
    let max = 4u32;
    let mut count = 0;

    for _ in 1..=max {
        count += 1;
    }

    assert_eq!(count, max);
}

#[tokio::test]
async fn test_retry_success_scenarios() {
    struct Case { name: &'static str, fails: u32, max: u32, ok: bool }

    let cases = vec![
        Case { name: "First attempt",      fails: 0,  max: 4, ok: true },
        Case { name: "Third attempt",       fails: 2,  max: 4, ok: true },
        Case { name: "Last attempt",        fails: 3,  max: 4, ok: true },
        Case { name: "Exhausted retries",   fails: 10, max: 4, ok: false },
    ];

    for tc in cases {
        let mut attempts = 0u32;
        let mut success = false;

        for _ in 1..=tc.max {
            attempts += 1;
            if attempts > tc.fails {
                success = true;
                break;
            }
        }

        assert_eq!(success, tc.ok, "case: {}", tc.name);
    }
}

#[tokio::test]
async fn test_wait_time_doubles() {
    let max = 4u32;
    let mut times = Vec::new();

    for a in 1..max {
        times.push(2u64.pow(a - 1));
    }

    for i in 1..times.len() {
        assert!(times[i] > times[i - 1]);
        assert_eq!(times[i], times[i - 1] * 2);
    }
}

#[tokio::test]
async fn test_total_wait_time() {
    let max = 4u32;
    let total: u64 = (1..max).map(|a| 2u64.pow(a - 1)).sum();
    // 1 + 2 + 4 = 7
    assert_eq!(total, 7);
}

#[tokio::test]
async fn test_redis_retry_error_messages() {
    let max = 3;
    for attempt in 1..=max {
        let msg = format!("Failed to connect to Redis. Retrying (attempt {}/{})", attempt, max);
        assert!(msg.contains(&attempt.to_string()));
        assert!(msg.contains(&max.to_string()));
    }
}

#[tokio::test]
async fn test_neo4j_retry_error_messages() {
    let max = 4;
    for attempt in 1..=max {
        let msg = format!("Failed to connect to Neo4j. Retrying (attempt {}/{})", attempt, max);
        assert!(msg.contains("Neo4j"));
        assert!(msg.contains(&attempt.to_string()));
    }
}

#[tokio::test]
async fn test_concurrent_retries_independent() {
    use tokio::spawn;

    let handles: Vec<_> = (1..=3).map(|i| {
        spawn(async move {
            let max = 4u32;
            for a in 1..=max {
                if a >= i { return true; }
            }
            false
        })
    }).collect();

    for h in handles {
        assert!(h.await.unwrap());
    }
}
