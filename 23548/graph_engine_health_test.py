"""
Graph Engine – Health Check Endpoint Tests
Tests: /api/health with Neo4j connectivity
"""
import pytest
from unittest.mock import patch
from fastapi.testclient import TestClient
import sys, os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "services", "graph-engine"))
from main import app

client = TestClient(app)


def test_health_check_success():
    with patch("main.neo4j_driver") as mock:
        mock.verify_connectivity.return_value = None
        r = client.get("/api/health")
    assert r.status_code == 200
    assert r.json() == {"status": "UP", "details": {"graph_engine": "UP", "neo4j": "UP"}}


def test_health_check_neo4j_failure():
    with patch("main.neo4j_driver") as mock:
        mock.verify_connectivity.side_effect = Exception("Connection refused")
        r = client.get("/api/health")
    assert r.status_code == 503
    data = r.json()
    assert data["detail"]["status"] == "DOWN"
    assert "DOWN" in data["detail"]["details"]["neo4j"]
    assert "Connection refused" in data["detail"]["details"]["neo4j"]


def test_health_check_no_driver():
    with patch("main.neo4j_driver", None):
        r = client.get("/api/health")
    assert r.status_code == 503
    assert r.json()["detail"]["status"] == "DOWN"
