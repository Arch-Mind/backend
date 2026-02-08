# Important Note About File Locations

## API Gateway (Go)
**Working Copy**: `apps/api-gateway/main_test.go`
**Presentation Copy**: `23548/main_test.go`

**Why**: Go requires test files (`*_test.go`) alongside source files.

---

## Ingestion Worker (Rust)
**Working Copy**: `services/ingestion-worker/src/tests.rs`
**Presentation Copy**: `23548/ingestion_worker_tests.rs`

**Why**: 
1. The test file uses `use super::*;` to access `ApiClient` and structs from `main.rs`.
2. It must be declared as a module in `main.rs` using `#[cfg(test)] mod tests;`.
3. Moving it outside `src` or renaming it without updating `main.rs` will break compilation.

**Integration**:
- `mock_api.py`: Python script to simulate API Gateway.
- Can be run anywhere, but worker must point to localhost:8080.

---

## Summary
The `23548` folder contains **copies** for presentation. To run tests, the files must exist in their respective service directories.
