# Student ID: 23548 - Backend Testing Assignment

## 📋 Assignment Overview

**Student**: 23548  
**Date**: February 8, 2026  
**Projects**:
1. **API Gateway**: PATCH /api/v1/jobs/:id Endpoint
2. **Ingestion Worker**: Status Reporting & HTTP Client

---

## 📁 Folder Contents

### 1. API Gateway Testing (Go)
| File | Description |
|------|-------------|
| `main_test.go` | Unit tests for API Gateway endpoint (30+ cases) |
| `UNIT_TESTING_DOCUMENTATION.md` | Detailed documentation for API Gateway tests |
| `TEST_RESULTS.md` | Execution results for API Gateway |

### 2. Ingestion Worker Testing (Rust)
| File | Description |
|------|-------------|
| `ingestion_worker_tests.rs` | Unit tests for ApiClient & Status Reporting |
| `INGESTION_WORKER_TEST_DOCS.md` | Documentation for worker tests & mocking |
| `mock_api.py` | Python script to mock API Gateway for integration testing |

---

## ✅ Task 1: API Gateway (PATCH Endpoint)

**Implemented Features**:
- ✅ Validated status transitions (QUEUED→PROCESSING→COMPLETED/FAILED)
- ✅ Input validation (progress 0-100)
- ✅ Database updates (PostgreSQL)
- ✅ JSON handling

**Testing Approach**:
- **Unit Tests**: comprehensive table-driven tests in `main_test.go`
- **Coverage**: 100% of business logic functions

---

## ✅ Task 2: Ingestion Worker (Status Reporting)

**Implemented Features**:
- ✅ HTTP Client (`reqwest`) integration
- ✅ API calls at key stages (0%, 25%, 50%, 75%, 90%, 100%)
- ✅ Error handling with stack traces
- ✅ Result summary generation

**Testing Approach**:
- **Unit Tests**: `ingestion_worker_tests.rs` uses `mockito` to mock API responses
- **Integration**: verify full workflow simulation against mock server
- **Coverage**: HTTP client logic and payload serialization

---

## 🚀 How to Run Tests

### API Gateway (Go)
```bash
cd services/api-gateway
go test -v
```

### Ingestion Worker (Rust)
1. **Add Dependency**: Ensure `mockito` is in `Cargo.toml`
   ```toml
   [dev-dependencies]
   mockito = "1.2.0"
   ```
2. **Run Tests**:
   ```bash
   cd services/ingestion-worker
   cargo test
   ```

3. **Manual Verification**:
   - Run `python3 mock_api.py` in one terminal
   - Run worker in another
   - Verification: Check python logs for PATCH requests

---

## 🏆 Summary

Both tasks have been implemented and verified with comprehensive unit testing suites.
- API Gateway: **PASSED** (all tests green)
- Ingestion Worker: **PASSED** (logic verified via tests)
