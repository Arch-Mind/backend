import pytest
from fastapi.testclient import TestClient
from unittest.mock import Mock, patch, MagicMock
import sys
import os

# Add parent directory to path for imports
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from main import (
    app, 
    validate_repo_id, 
    validate_pagination_params,
    check_repo_exists,
    get_total_count
)


client = TestClient(app)


# ================================
# Test 1: repo_id Validation
# ================================

def test_validate_repo_id_valid():
    """Test valid UUID format for repo_id."""
    valid_uuid = "550e8400-e29b-41d4-a716-446655440000"
    assert validate_repo_id(valid_uuid) == True


def test_validate_repo_id_invalid_format():
    """Test invalid UUID formats."""
    invalid_ids = [
        "not-a-uuid",
        "12345",
        "550e8400-e29b-41d4-a716",  # Incomplete
        "550e8400_e29b_41d4_a716_446655440000",  # Wrong separator
        "",  # Empty string
    ]
    for invalid_id in invalid_ids:
        assert validate_repo_id(invalid_id) == False


def test_validate_repo_id_case_insensitive():
    """Test UUID validation is case-insensitive."""
    uuid_upper = "550E8400-E29B-41D4-A716-446655440000"
    uuid_lower = "550e8400-e29b-41d4-a716-446655440000"
    assert validate_repo_id(uuid_upper) == True
    assert validate_repo_id(uuid_lower) == True


# ================================
# Test 2: Pagination Validation
# ================================

def test_validate_pagination_params_normal():
    """Test normal pagination parameters."""
    limit, offset = validate_pagination_params(50, 10)
    assert limit == 50
    assert offset == 10


def test_validate_pagination_params_clamping():
    """Test pagination parameter clamping."""
    # Test upper bound
    limit, offset = validate_pagination_params(5000, 0)
    assert limit == 1000  # Should clamp to max 1000
    
    # Test lower bound
    limit, offset = validate_pagination_params(0, -5)
    assert limit == 1  # Should clamp to min 1
    assert offset == 0  # Should clamp to non-negative


def test_validate_pagination_params_edge_cases():
    """Test edge cases for pagination."""
    # Maximum valid values
    limit, offset = validate_pagination_params(1000, 999999)
    assert limit == 1000
    assert offset == 999999
    
    # Minimum valid values
    limit, offset = validate_pagination_params(1, 0)
    assert limit == 1
    assert offset == 0


# ================================
# Test 3: Error Handling for Empty Results
# ================================

@patch('main.neo4j_driver')
def test_metrics_endpoint_repo_not_found(mock_driver):
    """Test metrics endpoint returns 404 when repo doesn't exist."""
    mock_session = MagicMock()
    mock_driver.session.return_value.__enter__.return_value = mock_session
    
    # Mock check_repo_exists to return False
    mock_result = MagicMock()
    mock_result.single.return_value = {"count": 0}
    mock_session.run.return_value = mock_result
    
    valid_uuid = "550e8400-e29b-41d4-a716-446655440000"
    response = client.get(f"/api/metrics/{valid_uuid}")
    
    assert response.status_code == 404
    assert "not found" in response.json()["detail"].lower()


@patch('main.neo4j_driver')
def test_graph_endpoint_repo_not_found(mock_driver):
    """Test graph endpoint returns 404 when repo doesn't exist."""
    mock_session = MagicMock()
    mock_driver.session.return_value.__enter__.return_value = mock_session
    
    # Mock check_repo_exists to return False
    mock_result = MagicMock()
    mock_result.single.return_value = {"count": 0}
    mock_session.run.return_value = mock_result
    
    valid_uuid = "550e8400-e29b-41d4-a716-446655440000"
    response = client.get(f"/api/graph/{valid_uuid}")
    
    assert response.status_code == 404
    assert "not found" in response.json()["detail"].lower()


@patch('main.neo4j_driver')
def test_metrics_endpoint_empty_data(mock_driver):
    """Test metrics endpoint handles empty dataset gracefully."""
    mock_session = MagicMock()
    mock_driver.session.return_value.__enter__.return_value = mock_session
    
    # Mock repo exists
    mock_exists_result = MagicMock()
    mock_exists_result.single.return_value = {"count": 1}
    
    # Mock all counts as 0
    mock_count_result = MagicMock()
    mock_count_result.single.return_value = {"count": 0}
    
    mock_session.run.side_effect = [
        mock_exists_result,  # check_repo_exists
        mock_count_result,   # files
        mock_count_result,   # functions
        mock_count_result,   # classes
        mock_count_result    # dependencies
    ]
    
    valid_uuid = "550e8400-e29b-41d4-a716-446655440000"
    response = client.get(f"/api/metrics/{valid_uuid}")
    
    assert response.status_code == 200
    data = response.json()
    assert data["total_files"] == 0
    assert data["total_functions"] == 0
    assert data["total_classes"] == 0
    assert data["complexity_score"] == 0.0


# ================================
# Test 4: Pagination in Graph Endpoint
# ================================

@patch('main.neo4j_driver')
def test_graph_endpoint_pagination(mock_driver):
    """Test graph endpoint supports pagination."""
    mock_session = MagicMock()
    mock_driver.session.return_value.__enter__.return_value = mock_session
    
    # Mock repo exists
    mock_exists_result = MagicMock()
    mock_exists_result.single.return_value = {"count": 1}
    
    # Mock counts
    mock_count_result = MagicMock()
    mock_count_result.single.return_value = {"count": 100}
    
    # Mock nodes and edges
    mock_nodes_result = MagicMock()
    mock_nodes_result.__iter__ = Mock(return_value=iter([
        {"id": "node1", "name": "test.py", "type": "File", "props": {}}
    ]))
    
    mock_edges_result = MagicMock()
    mock_edges_result.__iter__ = Mock(return_value=iter([]))
    
    mock_session.run.side_effect = [
        mock_exists_result,    # check_repo_exists
        mock_count_result,     # total nodes count
        mock_count_result,     # total edges count
        mock_nodes_result,     # nodes query
        mock_edges_result      # edges query
    ]
    
    valid_uuid = "550e8400-e29b-41d4-a716-446655440000"
    response = client.get(f"/api/graph/{valid_uuid}?limit=10&offset=0")
    
    assert response.status_code == 200
    data = response.json()
    assert "limit" in data
    assert "offset" in data
    assert "has_more" in data
    assert data["limit"] == 10
    assert data["offset"] == 0


@patch('main.neo4j_driver')
def test_graph_endpoint_has_more_flag(mock_driver):
    """Test has_more flag in pagination."""
    mock_session = MagicMock()
    mock_driver.session.return_value.__enter__.return_value = mock_session
    
    # Mock repo exists
    mock_exists_result = MagicMock()
    mock_exists_result.single.return_value = {"count": 1}
    
    # Mock counts - 100 total items
    mock_count_result = MagicMock()
    mock_count_result.single.return_value = {"count": 100}
    
    # Mock empty results for nodes/edges
    mock_empty_result = MagicMock()
    mock_empty_result.__iter__ = Mock(return_value=iter([]))
    
    mock_session.run.side_effect = [
        mock_exists_result,    # check_repo_exists
        mock_count_result,     # total nodes (100)
        mock_count_result,     # total edges (100)
        mock_empty_result,     # nodes query
        mock_empty_result      # edges query
    ]
    
    valid_uuid = "550e8400-e29b-41d4-a716-446655440000"
    
    # Request first page (0-10 of 100)
    response = client.get(f"/api/graph/{valid_uuid}?limit=10&offset=0")
    assert response.status_code == 200
    data = response.json()
    assert data["has_more"] == True  # 10 < 100, so more data available
    
    # Request last page would have has_more=False
    # (mocking this would require adjusting side_effect)


# ================================
# Test 5: Index Creation
# ================================

@patch('main.neo4j_driver')
def test_create_indexes_endpoint(mock_driver):
    """Test index creation endpoint."""
    mock_session = MagicMock()
    mock_driver.session.return_value.__enter__.return_value = mock_session
    
    # Mock successful index creation
    mock_session.run.return_value = None
    
    response = client.post("/api/admin/create-indexes")
    
    assert response.status_code == 200
    data = response.json()
    assert "indexes" in data
    assert "count" in data


# ================================
# Test 6: Invalid Input Validation
# ================================

def test_metrics_endpoint_invalid_repo_id():
    """Test metrics endpoint rejects invalid repo_id."""
    invalid_id = "not-a-valid-uuid"
    response = client.get(f"/api/metrics/{invalid_id}")
    
    assert response.status_code == 400
    assert "Invalid repo_id format" in response.json()["detail"]


def test_graph_endpoint_invalid_repo_id():
    """Test graph endpoint rejects invalid repo_id."""
    invalid_id = "12345"
    response = client.get(f"/api/graph/{invalid_id}")
    
    assert response.status_code == 400
    assert "Invalid repo_id format" in response.json()["detail"]


# ================================
# Test 7: Health Check
# ================================

def test_health_check_endpoint():
    """Test health check endpoint."""
    response = client.get("/health")
    assert response.status_code == 200
    data = response.json()
    assert "status" in data
    assert "services" in data


def test_root_endpoint():
    """Test root endpoint returns service info."""
    response = client.get("/")
    assert response.status_code == 200
    data = response.json()
    assert data["service"] == "ArchMind Graph Engine"
    assert "version" in data


# ================================
# Test 8: Neo4j Connection Handling
# ================================

def test_metrics_endpoint_neo4j_unavailable():
    """Test graceful handling when Neo4j is unavailable."""
    with patch('main.neo4j_driver', None):
        valid_uuid = "550e8400-e29b-41d4-a716-446655440000"
        response = client.get(f"/api/metrics/{valid_uuid}")
        
        assert response.status_code == 503
        assert "Neo4j connection not available" in response.json()["detail"]


# ================================
# Test 9: Workflow Simulation
# ================================

@patch('main.neo4j_driver')
def test_full_workflow_metrics_then_graph(mock_driver):
    """Test typical workflow: check metrics, then retrieve graph."""
    mock_session = MagicMock()
    mock_driver.session.return_value.__enter__.return_value = mock_session
    
    valid_uuid = "550e8400-e29b-41d4-a716-446655440000"
    
    # Step 1: Get metrics
    mock_exists = MagicMock()
    mock_exists.single.return_value = {"count": 1}
    
    mock_count = MagicMock()
    mock_count.single.return_value = {"count": 10}
    
    mock_session.run.side_effect = [
        mock_exists,  # repo exists
        mock_count,   # files
        mock_count,   # functions
        mock_count,   # classes
        mock_count    # deps
    ]
    
    response1 = client.get(f"/api/metrics/{valid_uuid}")
    assert response1.status_code == 200
    
    # Step 2: Get graph
    # Reset mock for next call
    mock_session.run.side_effect = [
        mock_exists,  # repo exists
        mock_count,   # total nodes
        mock_count,   # total edges
        MagicMock(__iter__=Mock(return_value=iter([]))),  # nodes
        MagicMock(__iter__=Mock(return_value=iter([])))   # edges
    ]
    
    response2 = client.get(f"/api/graph/{valid_uuid}?limit=10&offset=0")
    assert response2.status_code == 200


# ================================
# Run Tests
# ================================

if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
