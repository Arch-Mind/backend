use super::*;
use mockito::mock;
use serde_json::json;

#[tokio::test]
async fn test_api_client_update_job_success() {
    // Start a mock server
    let _m = mock("PATCH", "/api/v1/jobs/test-job-123")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status": "success"}"#)
        .create();

    // Initialize ApiClient with mock server URL
    let url = mockito::server_url();
    let client = ApiClient::new(url);

    // Create payload
    let payload = JobUpdatePayload {
        status: Some("PROCESSING".to_string()),
        progress: Some(10),
        result_summary: None,
        error: None,
    };

    // Execute update
    let result = client.update_job("test-job-123", payload).await;

    // Verify success
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_api_client_update_job_failure() {
    // Mock a 500 error
    let _m = mock("PATCH", "/api/v1/jobs/test-job-123")
        .with_status(500)
        .with_body("Internal Server Error")
        .create();

    let url = mockito::server_url();
    let client = ApiClient::new(url);

    let payload = JobUpdatePayload {
        status: Some("FAILED".to_string()),
        progress: None,
        result_summary: None,
        error: Some("Something went wrong".to_string()),
    };

    let result = client.update_job("test-job-123", payload).await;

    // Verify error
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "API Error: Internal Server Error");
}

#[tokio::test]
async fn test_job_update_payload_serialization() {
    let payload = JobUpdatePayload {
        status: Some("COMPLETED".to_string()),
        progress: Some(100),
        result_summary: Some(json!({"files": 10})),
        error: None,
    };

    let json = serde_json::to_string(&payload).expect("Failed to serialize");
    
    // Verify JSON structure
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse");
    assert_eq!(parsed["status"], "COMPLETED");
    assert_eq!(parsed["progress"], 100);
    assert_eq!(parsed["result_summary"]["files"], 10);
    assert!(parsed["error"].is_null());
}

#[tokio::test]
async fn test_api_client_full_workflow_simulation() {
    // Simulate the sequence of calls: 0% -> 25% -> 50% -> 75% -> 90% -> 100%
    
    let job_id = "workflow-job";
    let base_path = format!("/api/v1/jobs/{}", job_id);

    // 1. Initial Processing (0%)
    let _m1 = mock("PATCH", base_path.as_str())
        .match_body(mockito::Matcher::Json(json!({
            "status": "PROCESSING",
            "progress": 0
        })))
        .with_status(200)
        .create();
    
    // 2. Cloning (25%)
    let _m2 = mock("PATCH", base_path.as_str())
        .match_body(mockito::Matcher::Json(json!({
            "progress": 25
        })))
        .with_status(200)
        .create();

    // 3. Parsing (50%)
    let _m3 = mock("PATCH", base_path.as_str())
        .match_body(mockito::Matcher::Json(json!({
            "progress": 50
        })))
        .with_status(200)
        .create();

    // 4. Graph Building (75%)
    let _m4 = mock("PATCH", base_path.as_str())
        .match_body(mockito::Matcher::Json(json!({
            "progress": 75
        })))
        .with_status(200)
        .create();

    // 5. Storage (90%)
    let _m5 = mock("PATCH", base_path.as_str())
        .match_body(mockito::Matcher::Json(json!({
            "progress": 90
        })))
        .with_status(200)
        .create();

    // 6. Completion (100%)
    let _m6 = mock("PATCH", base_path.as_str())
        .match_body(mockito::Matcher::Json(json!({
            "status": "COMPLETED",
            "progress": 100,
            "result_summary": {"success": true}
        })))
        .with_status(200)
        .create();

    let url = mockito::server_url();
    let client = ApiClient::new(url);

    // Execute sequence
    client.update_job(job_id, JobUpdatePayload {
        status: Some("PROCESSING".to_string()),
        progress: Some(0),
        result_summary: None,
        error: None,
    }).await.expect("Step 1 failed");

    client.update_job(job_id, JobUpdatePayload {
        status: None,
        progress: Some(25),
        result_summary: None,
        error: None,
    }).await.expect("Step 2 failed");

    client.update_job(job_id, JobUpdatePayload {
        status: None,
        progress: Some(50),
        result_summary: None,
        error: None,
    }).await.expect("Step 3 failed");

    client.update_job(job_id, JobUpdatePayload {
        status: None,
        progress: Some(75),
        result_summary: None,
        error: None,
    }).await.expect("Step 4 failed");

    client.update_job(job_id, JobUpdatePayload {
        status: None,
        progress: Some(90),
        result_summary: None,
        error: None,
    }).await.expect("Step 5 failed");

    client.update_job(job_id, JobUpdatePayload {
        status: Some("COMPLETED".to_string()),
        progress: Some(100),
        result_summary: Some(json!({"success": true})),
        error: None,
    }).await.expect("Step 6 failed");
}
