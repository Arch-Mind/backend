import pytest
from fastapi.testclient import TestClient
from unittest.mock import patch

from main import app

client = TestClient(app)

def test_health_check_success():
    with patch("main.neo4j_driver") as mock_driver:
        mock_driver.verify_connectivity.return_value = None
        
        response = client.get("/api/health")
        assert response.status_code == 200
        assert response.json() == {"status": "UP", "details": {"graph_engine": "UP", "neo4j": "UP"}}

def test_health_check_neo4j_failure():
    with patch("main.neo4j_driver") as mock_driver:
        mock_driver.verify_connectivity.side_effect = Exception("Connection refused")
        
        response = client.get("/api/health")
        assert response.status_code == 503
        
        data = response.json()
        assert data["detail"]["status"] == "DOWN"
        assert "DOWN" in data["detail"]["details"]["neo4j"]
        assert "Connection refused" in data["detail"]["details"]["neo4j"]

def test_health_check_no_driver():
    with patch("main.neo4j_driver", None):
        response = client.get("/api/health")
        assert response.status_code == 503
        data = response.json()
        assert data["detail"]["status"] == "DOWN"
