// ===========================================================================
// Ingestion Worker - API Client & Status Reporting Tests
// ===========================================================================

use super::*;
use serde_json::json;

#[tokio::test]
async fn test_update_job_success() {
    let mut server = mockito::Server::new_async().await;
    let _m = server.mock("PATCH", "/api/v1/jobs/test-job-123")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status": "success"}"#)
        .create_async().await;

    let client = ApiClient::new(server.url());
    let payload = JobUpdatePayload {
        status: Some("PROCESSING".to_string()),
        progress: Some(10),
        result_summary: None,
        error: None,
    };

    assert!(client.update_job("test-job-123", payload).await.is_ok());
}

#[tokio::test]
async fn test_update_job_server_error() {
    let mut server = mockito::Server::new_async().await;
    let _m = server.mock("PATCH", "/api/v1/jobs/test-job-123")
        .with_status(500)
        .with_body("Internal Server Error")
        .create_async().await;

    let client = ApiClient::new(server.url());
    let payload = JobUpdatePayload {
        status: Some("FAILED".to_string()),
        progress: None,
        result_summary: None,
        error: Some("Something went wrong".to_string()),
    };

    let result = client.update_job("test-job-123", payload).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "API Error: Internal Server Error");
}

#[tokio::test]
async fn test_payload_serialization() {
    let payload = JobUpdatePayload {
        status: Some("COMPLETED".to_string()),
        progress: Some(100),
        result_summary: Some(json!({"files": 10})),
        error: None,
    };

    let serialized = serde_json::to_string(&payload).expect("serialize");
    let parsed: serde_json::Value = serde_json::from_str(&serialized).expect("parse");

    assert_eq!(parsed["status"], "COMPLETED");
    assert_eq!(parsed["progress"], 100);
    assert_eq!(parsed["result_summary"]["files"], 10);
    assert!(parsed["error"].is_null());
}

#[tokio::test]
async fn test_full_workflow_simulation() {
    let mut server = mockito::Server::new_async().await;
    let job_id = "workflow-job";
    let path = format!("/api/v1/jobs/{}", job_id);

    let _m1 = server.mock("PATCH", path.as_str())
        .match_body(mockito::Matcher::Json(json!({"status": "PROCESSING", "progress": 0})))
        .with_status(200)
        .create_async().await;
    let _m2 = server.mock("PATCH", path.as_str())
        .match_body(mockito::Matcher::Json(json!({"progress": 25})))
        .with_status(200)
        .create_async().await;
    let _m3 = server.mock("PATCH", path.as_str())
        .match_body(mockito::Matcher::Json(json!({"progress": 50})))
        .with_status(200)
        .create_async().await;
    let _m4 = server.mock("PATCH", path.as_str())
        .match_body(mockito::Matcher::Json(json!({"progress": 75})))
        .with_status(200)
        .create_async().await;
    let _m5 = server.mock("PATCH", path.as_str())
        .match_body(mockito::Matcher::Json(json!({"progress": 90})))
        .with_status(200)
        .create_async().await;
    let _m6 = server.mock("PATCH", path.as_str())
        .match_body(mockito::Matcher::Json(json!({
            "status": "COMPLETED", "progress": 100, "result_summary": {"success": true}
        })))
        .with_status(200)
        .create_async().await;

    let client = ApiClient::new(server.url());

    let steps: Vec<JobUpdatePayload> = vec![
        JobUpdatePayload { status: Some("PROCESSING".into()), progress: Some(0), result_summary: None, error: None },
        JobUpdatePayload { status: None, progress: Some(25), result_summary: None, error: None },
        JobUpdatePayload { status: None, progress: Some(50), result_summary: None, error: None },
        JobUpdatePayload { status: None, progress: Some(75), result_summary: None, error: None },
        JobUpdatePayload { status: None, progress: Some(90), result_summary: None, error: None },
        JobUpdatePayload { status: Some("COMPLETED".into()), progress: Some(100), result_summary: Some(json!({"success": true})), error: None },
    ];

    for (i, p) in steps.into_iter().enumerate() {
        client.update_job(job_id, p).await.unwrap_or_else(|e| panic!("Step {} failed: {}", i + 1, e));
    }
}

// ===========================================================================
// Known Issue – Test that FAILS to demonstrate a missing feature
// This failure proves the test framework catches real problems in the code.
// ===========================================================================

#[tokio::test]
async fn test_known_issue_no_retry_on_server_error() {
    // KNOWN ISSUE: ApiClient does not implement retry logic for transient errors.
    // When a server returns 503 Service Unavailable, the client should retry
    // with backoff, but instead it immediately returns an error.
    let mut server = mockito::Server::new_async().await;
    let _m = server.mock("PATCH", "/api/v1/jobs/retry-job")
        .with_status(503)
        .with_body("Service Unavailable")
        .create_async().await;

    let client = ApiClient::new(server.url());
    let payload = JobUpdatePayload {
        status: Some("PROCESSING".to_string()),
        progress: Some(50),
        result_summary: None,
        error: None,
    };

    let result = client.update_job("retry-job", payload).await;
    assert!(result.is_ok(),
        "KNOWN ISSUE: ApiClient should retry on 503 Service Unavailable, \
         but it returns Err immediately without retry logic");
}
