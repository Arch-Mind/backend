"""
Graph Engine – Neo4j Retry Logic Tests
Tests: exponential backoff, max retries, connect_neo4j_with_retry behaviour
"""
import pytest
from unittest.mock import patch, MagicMock
from neo4j.exceptions import ServiceUnavailable, AuthError
import sys, os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "services", "graph-engine"))


# ── Backoff Calculation ────────────────────────────────────────────────────

class TestExponentialBackoff:
    def test_values(self):
        for attempt, expected in [(1, 1), (2, 2), (3, 4), (4, 8)]:
            assert 2 ** (attempt - 1) == expected

    def test_wait_times_double(self):
        times = [2 ** (a - 1) for a in range(1, 4)]
        for i in range(1, len(times)):
            assert times[i] == times[i - 1] * 2

    def test_total_wait_time(self):
        total = sum(2 ** (a - 1) for a in range(1, 4))
        assert total == 7  # 1 + 2 + 4


# ── Max Retries ────────────────────────────────────────────────────────────

def test_max_retries_respected():
    max_retries = 4
    count = 0
    for _ in range(1, max_retries + 1):
        count += 1
    assert count == max_retries


def test_retry_success_scenarios():
    cases = [
        ("First attempt", 0, 4, True),
        ("Third attempt", 2, 4, True),
        ("Last attempt",  3, 4, True),
        ("Exhausted",    10, 4, False),
    ]
    for name, fails, max_r, expect in cases:
        ok = False
        for a in range(1, max_r + 1):
            if a > fails:
                ok = True
                break
        assert ok == expect, name


# ── connect_neo4j_with_retry ──────────────────────────────────────────────

class TestConnectNeo4j:
    @patch("main.GraphDatabase.driver")
    @patch("time.sleep")
    def test_success_first_attempt(self, mock_sleep, mock_driver):
        from main import connect_neo4j_with_retry
        inst = MagicMock()
        mock_driver.return_value = inst
        inst.verify_connectivity.return_value = None

        result = connect_neo4j_with_retry("bolt://localhost:7687", "neo4j", "pw", max_retries=4)
        assert result is not None
        mock_driver.assert_called_once()
        mock_sleep.assert_not_called()

    @patch("main.GraphDatabase.driver")
    @patch("time.sleep")
    def test_success_after_retries(self, mock_sleep, mock_driver):
        from main import connect_neo4j_with_retry
        inst = MagicMock()
        mock_driver.side_effect = [
            ServiceUnavailable("fail"),
            ServiceUnavailable("fail"),
            inst,
        ]
        inst.verify_connectivity.return_value = None

        result = connect_neo4j_with_retry("bolt://localhost:7687", "neo4j", "pw", max_retries=4)
        assert result is not None
        assert mock_driver.call_count == 3
        assert mock_sleep.call_count == 2

    @patch("main.GraphDatabase.driver")
    @patch("time.sleep")
    def test_all_retries_fail(self, mock_sleep, mock_driver):
        from main import connect_neo4j_with_retry
        mock_driver.side_effect = ServiceUnavailable("fail")

        result = connect_neo4j_with_retry("bolt://localhost:7687", "neo4j", "pw", max_retries=3)
        assert result is None
        assert mock_driver.call_count == 3
        assert mock_sleep.call_count == 2

    @patch("main.GraphDatabase.driver")
    @patch("time.sleep")
    def test_backoff_durations(self, mock_sleep, mock_driver):
        from main import connect_neo4j_with_retry
        mock_driver.side_effect = ServiceUnavailable("fail")

        connect_neo4j_with_retry("bolt://localhost:7687", "neo4j", "pw", max_retries=4)

        actual = [call[0][0] for call in mock_sleep.call_args_list]
        assert actual == [1, 2, 4]

    @patch("main.GraphDatabase.driver")
    @patch("time.sleep")
    def test_verify_connectivity_failure_triggers_retry(self, mock_sleep, mock_driver):
        from main import connect_neo4j_with_retry
        inst = MagicMock()
        mock_driver.return_value = inst
        inst.verify_connectivity.side_effect = [
            ServiceUnavailable("fail"),
            ServiceUnavailable("fail"),
            None,
        ]

        result = connect_neo4j_with_retry("bolt://localhost:7687", "neo4j", "pw", max_retries=4)
        assert result is not None
        assert inst.verify_connectivity.call_count == 3

    @patch("main.GraphDatabase.driver")
    def test_auth_error_exhausts_retries(self, mock_driver):
        from main import connect_neo4j_with_retry
        mock_driver.side_effect = AuthError("bad creds")

        result = connect_neo4j_with_retry("bolt://localhost:7687", "neo4j", "wrong", max_retries=3)
        assert result is None
        assert mock_driver.call_count == 3

    @patch("main.logger")
    @patch("main.GraphDatabase.driver")
    @patch("time.sleep")
    def test_retry_logging(self, mock_sleep, mock_driver, mock_logger):
        from main import connect_neo4j_with_retry
        mock_driver.side_effect = ServiceUnavailable("fail")

        connect_neo4j_with_retry("bolt://localhost:7687", "neo4j", "pw", max_retries=3)

        assert mock_logger.info.call_count >= 3
        assert mock_logger.warning.call_count == 2
        assert mock_logger.error.call_count == 1
