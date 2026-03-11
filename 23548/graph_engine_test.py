"""
Graph Engine – Feature Unit Tests
Tests: repo_id validation, pagination, 404 handling, endpoint integration
"""
import pytest
from unittest.mock import Mock, patch, MagicMock
from fastapi.testclient import TestClient
import sys, os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "services", "graph-engine"))
from main import app, validate_repo_id, validate_pagination_params

client = TestClient(app)


# ── repo_id Validation ─────────────────────────────────────────────────────

class TestValidateRepoId:
    def test_valid_uuid(self):
        assert validate_repo_id("550e8400-e29b-41d4-a716-446655440000") is True

    def test_invalid_formats(self):
        for bad in ["not-a-uuid", "12345", "550e8400-e29b-41d4-a716",
                     "550e8400_e29b_41d4_a716_446655440000", ""]:
            assert validate_repo_id(bad) is False, f"should reject {bad!r}"

    def test_case_insensitive(self):
        assert validate_repo_id("550E8400-E29B-41D4-A716-446655440000") is True
        assert validate_repo_id("550e8400-e29b-41d4-a716-446655440000") is True


# ── Pagination Validation ──────────────────────────────────────────────────

class TestPagination:
    def test_normal_values(self):
        limit, offset = validate_pagination_params(50, 10)
        assert limit == 50
        assert offset == 10

    def test_upper_clamp(self):
        limit, _ = validate_pagination_params(5000, 0)
        assert limit == 1000

    def test_lower_clamp(self):
        limit, offset = validate_pagination_params(0, -5)
        assert limit == 1
        assert offset == 0

    def test_boundary_values(self):
        limit, offset = validate_pagination_params(1000, 999999)
        assert limit == 1000
        assert offset == 999999

        limit, offset = validate_pagination_params(1, 0)
        assert limit == 1
        assert offset == 0


# ── 404 – Repo Not Found ──────────────────────────────────────────────────

def _mock_repo_missing(mock_driver):
    session = MagicMock()
    mock_driver.session.return_value.__enter__.return_value = session
    result = MagicMock()
    result.single.return_value = {"count": 0}
    session.run.return_value = result
    return session


class TestRepoNotFound:
    @patch("main.neo4j_driver")
    def test_metrics_404(self, mock_driver):
        _mock_repo_missing(mock_driver)
        r = client.get("/api/metrics/550e8400-e29b-41d4-a716-446655440000")
        assert r.status_code == 404
        assert "not found" in r.json()["detail"].lower()

    @patch("main.neo4j_driver")
    def test_graph_404(self, mock_driver):
        _mock_repo_missing(mock_driver)
        r = client.get("/api/graph/550e8400-e29b-41d4-a716-446655440000")
        assert r.status_code == 404
        assert "not found" in r.json()["detail"].lower()

    @patch("main.neo4j_driver")
    def test_graph_files_404(self, mock_driver):
        _mock_repo_missing(mock_driver)
        r = client.get("/api/graph/files?repo_id=550e8400-e29b-41d4-a716-446655440000")
        assert r.status_code == 404
        assert "not found" in r.json()["detail"].lower()


# ── Empty Data Handling ────────────────────────────────────────────────────

@patch("main.neo4j_driver")
def test_metrics_empty_dataset(mock_driver):
    session = MagicMock()
    mock_driver.session.return_value.__enter__.return_value = session

    exists = MagicMock(); exists.single.return_value = {"count": 1}
    zero   = MagicMock(); zero.single.return_value   = {"count": 0}

    session.run.side_effect = [exists, zero, zero, zero, zero]

    r = client.get("/api/metrics/550e8400-e29b-41d4-a716-446655440000")
    assert r.status_code == 200
    data = r.json()
    assert data["total_files"] == 0
    assert data["total_functions"] == 0
    assert data["total_classes"] == 0
    assert data["complexity_score"] == 0.0


# ── Pagination in Graph Endpoint ───────────────────────────────────────────

def _mock_paginated(mock_driver):
    session = MagicMock()
    mock_driver.session.return_value.__enter__.return_value = session

    exists = MagicMock(); exists.single.return_value = {"count": 1}
    count  = MagicMock(); count.single.return_value  = {"count": 100}
    nodes  = MagicMock(); nodes.__iter__ = Mock(return_value=iter([
        {"id": "n1", "name": "test.py", "type": "File", "props": {}}
    ]))
    edges = MagicMock(); edges.__iter__ = Mock(return_value=iter([]))

    session.run.side_effect = [exists, count, count, nodes, edges]
    return session


class TestGraphPagination:
    @patch("main.neo4j_driver")
    def test_pagination_fields(self, mock_driver):
        _mock_paginated(mock_driver)
        r = client.get("/api/graph/550e8400-e29b-41d4-a716-446655440000?limit=10&offset=0")
        assert r.status_code == 200
        data = r.json()
        assert data["limit"] == 10
        assert data["offset"] == 0
        assert "has_more" in data

    @patch("main.neo4j_driver")
    def test_has_more_flag(self, mock_driver):
        _mock_paginated(mock_driver)
        r = client.get("/api/graph/550e8400-e29b-41d4-a716-446655440000?limit=10&offset=0")
        assert r.json()["has_more"] is True

    @patch("main.neo4j_driver")
    def test_graph_files_pagination(self, mock_driver):
        _mock_paginated(mock_driver)
        r = client.get("/api/graph/files?repo_id=550e8400-e29b-41d4-a716-446655440000&limit=10&offset=0")
        assert r.status_code == 200
        data = r.json()
        assert "limit" in data
        assert "offset" in data
