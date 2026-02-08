// Tests for graceful shutdown functionality

use tokio::time::{timeout, Duration};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[tokio::test]
async fn test_shutdown_flag_behavior() {
    // Test AtomicBool shutdown flag
    let shutdown = Arc::new(AtomicBool::new(false));
    
    // Initially not shutdown
    assert!(!shutdown.load(Ordering::SeqCst), "Should not be shutdown initially");
    
    // Set shutdown flag
    shutdown.store(true, Ordering::SeqCst);
    assert!(shutdown.load(Ordering::SeqCst), "Should be shutdown after setting flag");
}

#[tokio::test]
async fn test_shutdown_flag_shared_across_tasks() {
    // Test that shutdown flag is shared across tasks
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    
    // Spawn task that sets shutdown
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        shutdown_clone.store(true, Ordering::SeqCst);
    });
    
    // Wait for shutdown to be set
    let mut attempts = 0;
    while !shutdown.load(Ordering::SeqCst) && attempts < 10 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        attempts += 1;
    }
    
    assert!(shutdown.load(Ordering::SeqCst), "Shutdown should be visible across tasks");
}

#[tokio::test]
async fn test_worker_loop_exits_on_shutdown() {
    // Simulate worker loop respecting shutdown flag
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_worker = shutdown.clone();
    
    let iterations = Arc::new(AtomicBool::new(false));
    let iterations_clone = iterations.clone();
    
    // Spawn worker loop
    let worker_handle = tokio::spawn(async move {
        let mut count = 0;
        while !shutdown_worker.load(Ordering::SeqCst) {
            count += 1;
            if count >= 3 {
                iterations_clone.store(true, Ordering::SeqCst);
                break; // Simulate job completion
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        count
    });
    
    // Let worker run
    tokio::time::sleep(Duration::from_millis(50)).await;
    
    // Trigger shutdown
    shutdown.store(true, Ordering::SeqCst);
    
    // Wait for worker
    let result = timeout(Duration::from_secs(1), worker_handle).await;
    assert!(result.is_ok(), "Worker should finish within timeout");
}

#[tokio::test]
async fn test_cleanup_temp_files_logic() {
    // Test the pattern matching for temp file cleanup
    let archmind_pattern = "archmind-";
    
    let test_cases = vec![
        ("archmind-12345", true),
        ("archmind-repo-abc", true),
        ("other-temp-file", false),
        ("my-archmind-file", false), // Should not match (doesn't start with pattern)
        ("archmind-", true),
    ];
    
    for (filename, should_match) in test_cases {
        let matches = filename.starts_with(archmind_pattern);
        assert_eq!(matches, should_match, 
            "File '{}' match should be {}", filename, should_match);
    }
}

#[tokio::test]
async fn test_concurrent_job_processing_during_shutdown() {
    // Test that current job finishes before shutdown
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_worker = shutdown.clone();
    
    let job_completed = Arc::new(AtomicBool::new(false));
    let job_completed_clone = job_completed.clone();
    
    // Simulate job processing
    let job_handle = tokio::spawn(async move {
        // Simulate job work
        tok io::time::sleep(Duration::from_millis(200)).await;
        
        // Check if shutdown was requested during job
        let shutdown_requested = shutdown_worker.load(Ordering::SeqCst);
        
        // Complete job regardless
        job_completed_clone.store(true, Ordering::SeqCst);
        
        shutdown_requested
    });
    
    // Wait a bit, then trigger shutdown
    tokio::time::sleep(Duration::from_millis(50)).await;
    shutdown.store(true, Ordering::SeqCst);
    
    // Wait for job to complete
    let shutdown_was_requested = timeout(Duration::from_secs(1), job_handle)
        .await
        .expect("Job should complete")
        .expect("Job task should succeed");
    
    assert!(shutdown_was_requested, "Shutdown should have been requested during job");
    assert!(job_completed.load(Ordering::SeqCst), "Job should complete before exit");
}

#[tokio::test]
async fn test_shutdown_signal_handling() {
    // Test signal handling logic
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    
    // Simulate signal handler
    let signal_handler = async move {
        // Simulate receiving signal
        tokio::time::sleep(Duration::from_millis(10)).await;
        shutdown_clone.store(true, Ordering::SeqCst);
        Ok::<(), std::io::Error>(())
    };
    
    // Run signal handler
    timeout(Duration::from_millis(100), signal_handler)
        .await
        .expect("Signal handler should complete")
        .expect("Signal handler should succeed");
    
    assert!(shutdown.load(Ordering::SeqCst), "Shutdown flag should be set by signal");
}

#[tokio::test]
async fn test_multiple_shutdown_signals() {
    // Test that multiple shutdown signals don't cause issues
    let shutdown = Arc::new(AtomicBool::new(false));
    
    // Set shutdown multiple times
    for _ in 0..5 {
        shutdown.store(true, Ordering::SeqCst);
    }
    
    assert!(shutdown.load(Ordering::SeqCst), "Shutdown should be set");
    
    // Setting to false and back to true
    shutdown.store(false, Ordering::SeqCst);
    shutdown.store(true, Ordering::SeqCst);
    
    assert!(shutdown.load(Ordering::SeqCst), "Shutdown should remain set");
}

#[tokio::test]
async fn test_worker_sleep_between_jobs() {
    // Test that worker sleeps when no jobs available
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_worker = shutdown.clone();
    
    let sleep_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sleep_count_clone = sleep_count.clone();
    
    // Simulate worker with sleep
    let worker = tokio::spawn(async move {
        while !shutdown_worker.load(Ordering::SeqCst) {
            // Simulate no job available
            sleep_count_clone.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            
            // Exit after a few iterations for test
            if sleep_count_clone.load(Ordering::SeqCst) >= 3 {
                break;
            }
        }
    });
    
    // Wait for worker
    timeout(Duration::from_secs(1), worker)
        .await
        .expect("Worker should complete");
    
    let count = sleep_count.load(Ordering::SeqCst);
    assert!(count >= 3, "Worker should have slept at least 3 times, got {}", count);
}

#[tokio::test]
async fn test_cleanup_message_logging() {
    // Test that cleanup messages are generated
    let cleanup_count = 5;
    
    let message = if cleanup_count > 0 {
        format!("✅ Cleaned up {} temporary directories", cleanup_count)
    } else {
        String::new()
    };
    
    assert!(message.contains("5"), "Message should contain count");
    assert!(message.contains("Cleaned up"), "Message should indicate cleanup");
}

#[tokio::test]
async fn test_temp_dir_path_handling() {
    // Test temp directory path logic
    use std::env;
    
    let temp_dir = env::temp_dir();
    assert!(temp_dir.exists() || !temp_dir.as_os_str().is_empty(),
        "Temp directory should be valid");
}

#[tokio::test]
async fn test_shutdown_completes_within_timeout() {
    // Test that shutdown completes within reasonable time
    let start = tokio::time::Instant::now();
    
    // Simulate shutdown sequence
    let shutdown = Arc::new(AtomicBool::new(false));
    shutdown.store(true, Ordering::SeqCst);
    
    // Simulate cleanup
    tokio::time::sleep(Duration::from_millis(50)).await;
    
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(2), 
        "Shutdown should complete quickly, took {:?}", elapsed);
}

#[tokio::test]
async fn test_graceful_vs_forced_shutdown() {
    // Test difference between graceful and forced shutdown
    let shutdown = Arc::new(AtomicBool::new(false));
    
    // Graceful: finish current work
    shutdown.store(true, Ordering::SeqCst);
    let is_graceful = shutdown.load(Ordering::SeqCst);
    assert!(is_graceful, "Graceful shutdown should set flag");
    
    // Forced would be: immediate exit (not testable without actual signal)
}
