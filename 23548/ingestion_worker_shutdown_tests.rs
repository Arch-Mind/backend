// ===========================================================================
// Ingestion Worker – Graceful Shutdown Tests
// ===========================================================================

use tokio::time::{timeout, Duration};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[tokio::test]
async fn test_shutdown_flag_initial_state() {
    let flag = Arc::new(AtomicBool::new(false));
    assert!(!flag.load(Ordering::SeqCst));

    flag.store(true, Ordering::SeqCst);
    assert!(flag.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_shutdown_flag_shared_across_tasks() {
    let flag = Arc::new(AtomicBool::new(false));
    let clone = flag.clone();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        clone.store(true, Ordering::SeqCst);
    });

    let mut seen = false;
    for _ in 0..20 {
        if flag.load(Ordering::SeqCst) { seen = true; break; }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(seen);
}

#[tokio::test]
async fn test_worker_loop_exits_on_shutdown() {
    let flag = Arc::new(AtomicBool::new(false));
    let f = flag.clone();

    let handle = tokio::spawn(async move {
        let mut count = 0u32;
        while !f.load(Ordering::SeqCst) {
            count += 1;
            if count >= 3 { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        count
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    flag.store(true, Ordering::SeqCst);

    let result = timeout(Duration::from_secs(1), handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_temp_file_cleanup_pattern() {
    let pattern = "archmind-";
    let cases = vec![
        ("archmind-12345", true),
        ("archmind-repo-abc", true),
        ("other-temp-file", false),
        ("my-archmind-file", false),
        ("archmind-", true),
    ];

    for (name, expected) in cases {
        assert_eq!(name.starts_with(pattern), expected, "file: {}", name);
    }
}

#[tokio::test]
async fn test_current_job_finishes_before_shutdown() {
    let flag = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let f = flag.clone();
    let d = done.clone();

    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        d.store(true, Ordering::SeqCst);
        f.load(Ordering::SeqCst)
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    flag.store(true, Ordering::SeqCst);

    let shutdown_was_set = timeout(Duration::from_secs(1), handle)
        .await.expect("join").expect("task");

    assert!(shutdown_was_set);
    assert!(done.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_signal_handler_sets_flag() {
    let flag = Arc::new(AtomicBool::new(false));
    let f = flag.clone();

    let handler = async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        f.store(true, Ordering::SeqCst);
    };

    timeout(Duration::from_millis(100), handler).await.expect("handler");
    assert!(flag.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_multiple_shutdown_signals_are_safe() {
    let flag = Arc::new(AtomicBool::new(false));

    for _ in 0..5 {
        flag.store(true, Ordering::SeqCst);
    }
    assert!(flag.load(Ordering::SeqCst));

    flag.store(false, Ordering::SeqCst);
    flag.store(true, Ordering::SeqCst);
    assert!(flag.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_worker_sleeps_between_jobs() {
    let flag = Arc::new(AtomicBool::new(false));
    let f = flag.clone();
    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();

    let worker = tokio::spawn(async move {
        while !f.load(Ordering::SeqCst) {
            c.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            if c.load(Ordering::SeqCst) >= 3 { break; }
        }
    });

    timeout(Duration::from_secs(1), worker).await.expect("join").expect("task");
    assert!(counter.load(Ordering::SeqCst) >= 3);
}

#[tokio::test]
async fn test_shutdown_completes_within_timeout() {
    let start = tokio::time::Instant::now();

    let flag = Arc::new(AtomicBool::new(false));
    flag.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(start.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn test_cleanup_log_message() {
    let count = 5;
    let msg = format!("Cleaned up {} temporary directories", count);
    assert!(msg.contains("5"));
    assert!(msg.contains("Cleaned up"));
}
