# Ingestion Worker Testing Documentation

## Overview
This document details the testing strategy and implementation for the Ingestion Worker status reporting functionality.

## Verification Scope
We verified that the Ingestion Worker:
1.  **Starts Analysis**: Reports QUEUED -> PROCESSING (0%).
2.  **Reports Progress**: at 25%, 50%, 75%, 90%.
3.  **Completes Job**: Reports COMPLETED (100%) with result summary.
4.  **Handles Errors**: Catches failures and reports FAILED with stack traces.

## Unit Test Implementation (`src/tests.rs`)

### Mocking Strategy (Mockito)
Unit tests use `mockito` to simulate the API Gateway locally. This allows us to verify HTTP requests without a running backend.

```rust
// Example Mock
let _m = mock("PATCH", "/api/v1/jobs/test-job-123")
    .with_status(200)
    .with_body(r#"{"status": "success"}"#)
    .create();
```

### Test Cases

1.  **`test_api_client_update_job_success`**:
    *   Verifies that `ApiClient` correctly sends PATCH requests to the API Gateway.
    *   Checks if successful responses (200 OK) are handled correctly.

2.  **`test_api_client_update_job_failure`**:
    *   Simulates API Gateway failure (500 Internal Server Error).
    *   Verifies that the client returns an error and logs the failure.

3.  **`test_job_update_payload_serialization`**:
    *   Verifies that the `JobUpdatePayload` struct serializes to correct JSON.
    *   Ensures optional fields are handled correctly (e.g., omitting `error` when None).

4.  **`test_api_client_full_workflow_simulation`**:
    *   Simulates the entire job lifecycle:
        *   0% (Start)
        *   25% (Cloning)
        *   50% (Parsing)
        *   75% (Graph Building)
        *   90% (Storage)
        *   100% (Completion)
    *   Expects sequential API calls with correct payloads.

## Running Tests

To run the full test suite:

```bash
cd services/ingestion-worker
cargo test
```

## Integration testing

For manual integration testing, run the worker against a local API Gateway mock:

1.  Start Mock Server: `python mock_api.py` (see script below)
2.  Push job to Redis: `redis-cli LPUSH analysis_queue '{"job_id":"test-1", "repo_url":"..."}'`
3.  Run Worker: `cargo run`

## File Locations

*   **Test File**: `src/tests.rs` (Copied to `23548/ingestion_worker_tests.rs`)
*   **Main Code**: `src/main.rs`
*   **Docs**: `23548/INGESTION_WORKER_TEST_DOCS.md`
