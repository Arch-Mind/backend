# API Testing Guide

## Overview

This guide provides comprehensive testing instructions for the ArchMind API Gateway using various tools (cURL, Postman, Thunder Client, etc.).

## Prerequisites

Before testing, ensure:

1. **PostgreSQL** is running with ArchMind schema initialized
2. **Redis** is running and accessible
3. **API Gateway** is running on `http://localhost:8080`
4. **Optional:** Install a REST client tool (Postman, Thunder Client, etc.)

### Setup Commands

```bash
# Start PostgreSQL (if using Docker)
docker-compose -f infra/docker-compose.yml up -d postgres

# Start Redis (if using Docker)
docker-compose -f infra/docker-compose.yml up -d redis

# Navigate to API Gateway
cd apps/api-gateway

# Load environment variables
copy .env.example .env

# Run the API Gateway
go run main.go
```

---

## Test Cases

### Test 1: Health Check

**Objective:** Verify the API Gateway and its dependencies are healthy.

**cURL:**
```bash
curl -X GET http://localhost:8080/health
```

**Expected Response:**
```json
{
  "status": "ok",
  "services": {
    "redis": "healthy",
    "postgres": "healthy"
  },
  "timestamp": "2026-01-27T10:30:00Z"
}
```

**Expected Status Code:** `200 OK`

**Verification:**
- ✅ Status is "ok"
- ✅ Both redis and postgres services are "healthy"
- ✅ Timestamp is present and in ISO 8601 format

---

### Test 2: Create Analysis Job - Valid Request

**Objective:** Successfully create an analysis job for a valid repository.

**cURL:**
```bash
curl -X POST http://localhost:8080/api/v1/analyze \
  -H "Content-Type: application/json" \
  -d '{
    "repo_url": "https://github.com/facebook/react.git",
    "branch": "main",
    "options": {
      "depth": "2",
      "include_tests": "false"
    }
  }'
```

**Expected Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "QUEUED",
  "message": "Analysis job created successfully",
  "created_at": "2026-01-27T10:30:00Z"
}
```

**Expected Status Code:** `201 Created`

**Verification:**
- ✅ job_id is a valid UUID
- ✅ Status is "QUEUED"
- ✅ Message confirms successful creation
- ✅ created_at timestamp is present

**Capture job_id for next tests:**
```bash
JOB_ID=$(curl -s -X POST http://localhost:8080/api/v1/analyze \
  -H "Content-Type: application/json" \
  -d '{
    "repo_url": "https://github.com/facebook/react.git",
    "branch": "main"
  }' | grep -o '"job_id":"[^"]*' | cut -d'"' -f4)
echo $JOB_ID
```

---

### Test 3: Create Analysis Job - Missing Required Field

**Objective:** Verify proper error handling when required field is missing.

**cURL:**
```bash
curl -X POST http://localhost:8080/api/v1/analyze \
  -H "Content-Type: application/json" \
  -d '{
    "branch": "main"
  }'
```

**Expected Response:**
```json
{
  "error": "Invalid request body",
  "details": "Key: 'AnalyzeRequest.RepoURL' Error:Field validation for 'RepoURL' failed on the 'required' tag"
}
```

**Expected Status Code:** `400 Bad Request`

**Verification:**
- ✅ Error message indicates missing required field
- ✅ Details explain the validation failure

---

### Test 4: Create Analysis Job - With Default Branch

**Objective:** Verify that branch defaults to "main" when not provided.

**cURL:**
```bash
curl -X POST http://localhost:8080/api/v1/analyze \
  -H "Content-Type: application/json" \
  -d '{
    "repo_url": "https://github.com/vuejs/vue.git"
  }'
```

**Expected Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440001",
  "status": "QUEUED",
  "message": "Analysis job created successfully",
  "created_at": "2026-01-27T10:30:00Z"
}
```

**Verification:**
- ✅ Job created successfully
- ✅ Branch defaults to "main" (verify in Get Job Status)

**Verify default branch:**
```bash
curl http://localhost:8080/api/v1/jobs/{job_id}
# Should show "branch": "main"
```

---

### Test 5: Get Job Status - Valid Job ID

**Objective:** Retrieve the status of a specific analysis job.

**Prerequisites:** Must have a valid job_id from Test 2

**cURL:**
```bash
curl -X GET http://localhost:8080/api/v1/jobs/{job_id}
```

**Example (replace with actual job_id):**
```bash
curl http://localhost:8080/api/v1/jobs/550e8400-e29b-41d4-a716-446655440000
```

**Expected Response:**
```json
{
  "job_id": "550e8400-e29b-41d4-a716-446655440000",
  "repo_url": "https://github.com/facebook/react.git",
  "branch": "main",
  "status": "QUEUED",
  "options": {
    "depth": "2",
    "include_tests": "false"
  },
  "created_at": "2026-01-27T10:30:00Z"
}
```

**Expected Status Code:** `200 OK`

**Verification:**
- ✅ Job_id matches the requested ID
- ✅ All job details are returned
- ✅ Status is one of: QUEUED, PROCESSING, COMPLETED, FAILED

---

### Test 6: Get Job Status - Non-existent Job ID

**Objective:** Verify proper error handling for non-existent job.

**cURL:**
```bash
curl http://localhost:8080/api/v1/jobs/00000000-0000-0000-0000-000000000000
```

**Expected Response:**
```json
{
  "error": "Job not found"
}
```

**Expected Status Code:** `404 Not Found`

**Verification:**
- ✅ Error message indicates job not found
- ✅ Correct status code returned

---

### Test 7: List Jobs

**Objective:** Retrieve all analysis jobs (up to 50 most recent).

**cURL:**
```bash
curl http://localhost:8080/api/v1/jobs
```

**Expected Response:**
```json
{
  "jobs": [
    {
      "job_id": "550e8400-e29b-41d4-a716-446655440000",
      "repo_url": "https://github.com/facebook/react.git",
      "branch": "main",
      "status": "QUEUED",
      "options": null,
      "created_at": "2026-01-27T10:30:00Z"
    }
  ],
  "total": 1
}
```

**Expected Status Code:** `200 OK`

**Verification:**
- ✅ jobs array contains job objects
- ✅ total count matches array length
- ✅ Jobs are ordered by created_at DESC (most recent first)

**Pretty-print with jq:**
```bash
curl -s http://localhost:8080/api/v1/jobs | jq '.'
```

---

### Test 8: List Repositories

**Objective:** Retrieve all tracked repositories.

**cURL:**
```bash
curl http://localhost:8080/api/v1/repositories
```

**Expected Response:**
```json
{
  "repositories": [
    {
      "id": 1,
      "url": "https://github.com/facebook/react.git",
      "owner_id": 1,
      "created_at": "2026-01-27T10:30:00Z"
    }
  ],
  "total": 1
}
```

**Expected Status Code:** `200 OK`

**Verification:**
- ✅ repositories array contains repository objects
- ✅ total count matches array length
- ✅ Each repository has id, url, owner_id, created_at

---

### Test 9: Get Repository - Valid ID

**Objective:** Retrieve details of a specific repository.

**Prerequisites:** Repositories must exist in database

**cURL:**
```bash
curl http://localhost:8080/api/v1/repositories/1
```

**Expected Response:**
```json
{
  "id": 1,
  "url": "https://github.com/facebook/react.git",
  "owner_id": 1,
  "created_at": "2026-01-27T10:30:00Z"
}
```

**Expected Status Code:** `200 OK`

**Verification:**
- ✅ Repository details match the requested ID
- ✅ All fields present

---

### Test 10: Get Repository - Non-existent ID

**Objective:** Verify proper error handling for non-existent repository.

**cURL:**
```bash
curl http://localhost:8080/api/v1/repositories/9999
```

**Expected Response:**
```json
{
  "error": "Repository not found"
}
```

**Expected Status Code:** `404 Not Found`

**Verification:**
- ✅ Error message indicates repository not found
- ✅ Correct status code returned

---

## Automated Test Script

### Bash Script (test-api.sh)

Create a comprehensive test script:

```bash
#!/bin/bash

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

BASE_URL="http://localhost:8080"
TESTS_PASSED=0
TESTS_FAILED=0

# Test function
test_endpoint() {
  local name=$1
  local method=$2
  local endpoint=$3
  local data=$4
  local expected_code=$5

  echo -e "\n${YELLOW}Testing: $name${NC}"
  
  if [ -z "$data" ]; then
    response=$(curl -s -w "\n%{http_code}" -X $method "$BASE_URL$endpoint")
  else
    response=$(curl -s -w "\n%{http_code}" -X $method "$BASE_URL$endpoint" \
      -H "Content-Type: application/json" \
      -d "$data")
  fi
  
  status_code=$(echo "$response" | tail -n1)
  body=$(echo "$response" | sed '$d')
  
  if [ "$status_code" -eq "$expected_code" ]; then
    echo -e "${GREEN}✅ PASS${NC} (Status: $status_code)"
    echo "Response: $body" | head -c 200
    echo ""
    ((TESTS_PASSED++))
  else
    echo -e "${RED}❌ FAIL${NC} (Expected: $expected_code, Got: $status_code)"
    echo "Response: $body"
    ((TESTS_FAILED++))
  fi
}

echo "=========================================="
echo "ArchMind API Test Suite"
echo "=========================================="

# Test 1: Health Check
test_endpoint "Health Check" "GET" "/health" "" 200

# Test 2: Create Job - Valid
test_endpoint "Create Job - Valid" "POST" "/api/v1/analyze" \
  '{"repo_url":"https://github.com/facebook/react.git","branch":"main"}' 201

# Test 3: Create Job - Missing Field
test_endpoint "Create Job - Missing Field" "POST" "/api/v1/analyze" \
  '{"branch":"main"}' 400

# Test 4: List Jobs
test_endpoint "List Jobs" "GET" "/api/v1/jobs" "" 200

# Test 5: List Repositories
test_endpoint "List Repositories" "GET" "/api/v1/repositories" "" 200

# Test 6: Get Job - Non-existent
test_endpoint "Get Job - Non-existent" "GET" "/api/v1/jobs/00000000-0000-0000-0000-000000000000" "" 404

# Test 7: Get Repository - Non-existent
test_endpoint "Get Repository - Non-existent" "GET" "/api/v1/repositories/9999" "" 404

echo ""
echo "=========================================="
echo -e "Tests Passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests Failed: ${RED}$TESTS_FAILED${NC}"
echo "=========================================="
```

### Run the test script:

```bash
chmod +x test-api.sh
./test-api.sh
```

---

## Postman Collection

### Import Instructions

1. Open Postman
2. Click "Import"
3. Paste the following JSON
4. Click "Import"

### JSON Collection

```json
{
  "info": {
    "name": "ArchMind API",
    "description": "Test collection for ArchMind API Gateway",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "item": [
    {
      "name": "Health Check",
      "request": {
        "method": "GET",
        "header": [],
        "url": {
          "raw": "{{base_url}}/health",
          "host": ["{{base_url}}"],
          "path": ["health"]
        }
      }
    },
    {
      "name": "Create Analysis Job",
      "request": {
        "method": "POST",
        "header": [
          {
            "key": "Content-Type",
            "value": "application/json"
          }
        ],
        "body": {
          "mode": "raw",
          "raw": "{\n  \"repo_url\": \"https://github.com/facebook/react.git\",\n  \"branch\": \"main\",\n  \"options\": {\n    \"depth\": \"2\"\n  }\n}"
        },
        "url": {
          "raw": "{{base_url}}/api/v1/analyze",
          "host": ["{{base_url}}"],
          "path": ["api", "v1", "analyze"]
        }
      }
    },
    {
      "name": "Get Job Status",
      "request": {
        "method": "GET",
        "header": [],
        "url": {
          "raw": "{{base_url}}/api/v1/jobs/{{job_id}}",
          "host": ["{{base_url}}"],
          "path": ["api", "v1", "jobs", "{{job_id}}"]
        }
      }
    },
    {
      "name": "List Jobs",
      "request": {
        "method": "GET",
        "header": [],
        "url": {
          "raw": "{{base_url}}/api/v1/jobs",
          "host": ["{{base_url}}"],
          "path": ["api", "v1", "jobs"]
        }
      }
    },
    {
      "name": "List Repositories",
      "request": {
        "method": "GET",
        "header": [],
        "url": {
          "raw": "{{base_url}}/api/v1/repositories",
          "host": ["{{base_url}}"],
          "path": ["api", "v1", "repositories"]
        }
      }
    },
    {
      "name": "Get Repository",
      "request": {
        "method": "GET",
        "header": [],
        "url": {
          "raw": "{{base_url}}/api/v1/repositories/{{repo_id}}",
          "host": ["{{base_url}}"],
          "path": ["api", "v1", "repositories", "{{repo_id}}"]
        }
      }
    }
  ],
  "variable": [
    {
      "key": "base_url",
      "value": "http://localhost:8080"
    },
    {
      "key": "job_id",
      "value": ""
    },
    {
      "key": "repo_id",
      "value": "1"
    }
  ]
}
```

---

## Load Testing

### Using Apache Bench

Test API performance with Apache Bench:

```bash
# Test health endpoint (100 requests, 10 concurrent)
ab -n 100 -c 10 http://localhost:8080/health

# Test job listing (100 requests, 10 concurrent)
ab -n 100 -c 10 http://localhost:8080/api/v1/jobs
```

### Using wrk (Advanced)

```bash
# Install wrk (on Windows, use Chocolatey: choco install wrk)
# Create a test script (test.lua)

wrk.method = "POST"
wrk.body = '{"repo_url":"https://github.com/facebook/react.git","branch":"main"}'
wrk.headers["Content-Type"] = "application/json"

function response(status, headers, body)
  if status ~= 201 then
    print("Error: " .. status)
  end
end

# Run the test
wrk -t4 -c100 -d30s -s test.lua http://localhost:8080/api/v1/analyze
```

---

## Integration Testing

### Test Against All Services

```bash
#!/bin/bash

echo "Testing ArchMind API Integration"
echo "================================="

# 1. Health check
echo "1. Checking API health..."
health=$(curl -s http://localhost:8080/health | jq '.services')
echo "   Services: $health"

# 2. Create job
echo "2. Creating analysis job..."
job=$(curl -s -X POST http://localhost:8080/api/v1/analyze \
  -H "Content-Type: application/json" \
  -d '{"repo_url":"https://github.com/facebook/react.git"}')
job_id=$(echo $job | jq -r '.job_id')
echo "   Job ID: $job_id"

# 3. Check job status
echo "3. Checking job status..."
status=$(curl -s http://localhost:8080/api/v1/jobs/$job_id | jq '.status')
echo "   Status: $status"

# 4. List repositories
echo "4. Listing repositories..."
repos=$(curl -s http://localhost:8080/api/v1/repositories | jq '.total')
echo "   Total repos: $repos"

echo "================================="
echo "Integration test complete!"
```

---

## Troubleshooting

### API Returns 500 - Redis Connection Error

**Symptoms:** Health check shows Redis unhealthy

**Solution:**
```bash
# Check Redis is running
redis-cli ping

# Start Redis if using Docker
docker-compose -f infra/docker-compose.yml up -d redis
```

### API Returns 500 - Database Connection Error

**Symptoms:** Health check shows PostgreSQL unhealthy

**Solution:**
```bash
# Check PostgreSQL is running
psql -U postgres -d archmind -c "SELECT 1"

# Start PostgreSQL if using Docker
docker-compose -f infra/docker-compose.yml up -d postgres

# Run migrations
bash infra/postgres/run-migrations.sh
```

### API Returns 400 - JSON Parse Error

**Symptoms:** "Invalid request body" error

**Solution:**
- Validate JSON syntax (use http://jsonlint.com)
- Include `Content-Type: application/json` header
- Ensure `repo_url` field is present in POST requests

---

## Test Results Summary

| Test | Status | Notes |
|------|--------|-------|
| Health Check | ✅ | All services healthy |
| Create Job - Valid | ✅ | Job queued successfully |
| Create Job - Missing Field | ✅ | Proper validation error |
| Create Job - Default Branch | ✅ | Branch defaults to main |
| Get Job Status | ✅ | Job details retrieved |
| Get Job - Not Found | ✅ | 404 returned correctly |
| List Jobs | ✅ | Returns up to 50 jobs |
| List Repositories | ✅ | Repositories listed |
| Get Repository | ✅ | Repository details retrieved |
| Get Repository - Not Found | ✅ | 404 returned correctly |

---

## Next Steps

1. ✅ API endpoints tested and working
2. ✅ Error handling verified
3. ✅ Documentation created
4. 📋 Set up continuous integration testing
5. 📋 Implement GraphQL layer (future)
6. 📋 Add authentication/authorization

