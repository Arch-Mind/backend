# API Gateway Validation Testing Documentation

## Overview
This document details the unit testing of input validation logic in the API Gateway.

## ✅ Verified Requirements

### 1. Repository URL Validation
**Implementation**: `validateRepoURL` (Regex)
- Supports HTTPS, SSH, and Git protocols
- Validates structure (host, path, optional .git)
- Rejects invalid characters and empty strings

**Tests**:
- `TestValidateRepoURL`: 12 test cases covering valid/invalid/malicious URLs

### 2. Branch Name Safety
**Implementation**: `validateBranchName` (Regex + Logic)
- Allows alphanumeric, dashes, dots, slashes
- Rejects directory traversal ("..")
- Rejects dangerous characters (Shell injection prevention)

**Tests**:
- `TestValidateBranchName`: 18 test cases including injection attempts

### 3. UUID Validation
**Implementation**: `validateUUID` (uuid.Parse)
- Validates job IDs and repository IDs
- Ensures strict UUID format

**Tests**:
- `TestValidateUUID`: 7 test cases

### 4. API Error Responses
**Implementation**: `validationError` helper
- Returns 400 Bad Request
- Structured JSON: `{ "error": "...", "field": "...", "message": "..." }`

**Tests**:
- `TestAnalyzeRepository_Validation`
- `TestGetJobStatus_Validation`
- `TestUpdateJob_Validation`

---

## 🧪 Test Coverage

**File**: `api_gateway_validation_test.go`
- **Unit Tests**: 37+ validation scenarios
- **Integration Tests**: 3 handler tests verifying HTTP 400 responses
- **Total Coverage**: All validation paths covered

## 🚀 How to Run
```bash
cd apps/api-gateway
go test -v validation_test.go
```

**Student ID**: 23548
**Status**: ✅ Validated
