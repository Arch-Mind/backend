"""
Graph Engine - Known Issues Tests
Tests that FAIL to demonstrate edge cases and validation gaps.
These failures prove the test framework catches real problems in the code.
"""
import pytest
from unittest.mock import patch, MagicMock
from fastapi.testclient import TestClient
import sys, os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "services", "graph-engine"))
from main import app, validate_pagination_params

client = TestClient(app)


class TestKnownIssues:
    def test_negative_limit_should_raise_error(self):
        """KNOWN ISSUE: validate_pagination_params silently clamps negative limits to 1
        instead of raising a validation error. A strict API should reject invalid input
        rather than silently correcting it."""
        with pytest.raises(ValueError, match="Limit must be positive"):
            validate_pagination_params(-10, 0)

    @patch("main.neo4j_driver", MagicMock())
    def test_invalid_repo_id_should_have_field_name(self):
        """KNOWN ISSUE: Invalid repo_id returns 400 but the error response lacks
        a 'field' key to tell the client WHICH parameter was invalid.
        REST API best practice is to include field-level error details."""
        r = client.get("/api/metrics/not-a-valid-uuid")
        assert r.status_code == 400
        data = r.json()
        assert "field" in data, (
            "KNOWN ISSUE: Error response is missing 'field' key. "
            f"Got keys: {list(data.keys())}. "
            "API should return field-level validation errors for better client debugging."
        )
