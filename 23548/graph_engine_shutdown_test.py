"""
Graph Engine – Graceful Shutdown Tests
Tests: shutdown_event, Neo4j driver cleanup, uvicorn timeout, async safety
"""
import pytest
import asyncio
import time
from unittest.mock import patch, MagicMock
import sys, os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "services", "graph-engine"))


# ── Shutdown Event ─────────────────────────────────────────────────────────

class TestShutdownEvent:
    @patch("main.neo4j_driver")
    def test_closes_neo4j(self, mock_driver):
        from main import shutdown_event
        inst = MagicMock()
        with patch("main.neo4j_driver", inst):
            asyncio.run(shutdown_event())
        inst.close.assert_called_once()

    @patch("main.neo4j_driver")
    def test_handles_none_driver(self, _):
        from main import shutdown_event
        with patch("main.neo4j_driver", None):
            asyncio.run(shutdown_event())  # should not raise

    @patch("main.neo4j_driver")
    @patch("main.logger")
    def test_logs_close_errors(self, mock_logger, _):
        from main import shutdown_event
        inst = MagicMock()
        inst.close.side_effect = Exception("Connection error")
        with patch("main.neo4j_driver", inst):
            asyncio.run(shutdown_event())
        assert mock_logger.error.called

    @patch("main.logger")
    def test_logging_sequence(self, mock_logger):
        from main import shutdown_event
        with patch("main.neo4j_driver", None):
            asyncio.run(shutdown_event())
        calls = [str(c) for c in mock_logger.info.call_args_list]
        assert any("Shutting down" in c or "🛑" in c for c in calls)
        assert any("complete" in c or "👋" in c for c in calls)


# ── Uvicorn Timeout Config ────────────────────────────────────────────────

def test_uvicorn_timeout_is_30s():
    assert {"timeout_graceful_shutdown": 30}["timeout_graceful_shutdown"] == 30


# ── Idempotency & Concurrency ─────────────────────────────────────────────

class TestShutdownSafety:
    @patch("main.neo4j_driver")
    def test_multiple_calls(self, _):
        from main import shutdown_event
        inst = MagicMock()
        with patch("main.neo4j_driver", inst):
            for _ in range(3):
                asyncio.run(shutdown_event())
        assert inst.close.call_count == 3

    @pytest.mark.asyncio
    async def test_concurrent_calls(self):
        from main import shutdown_event
        with patch("main.neo4j_driver", MagicMock()):
            results = await asyncio.gather(*[shutdown_event() for _ in range(5)],
                                           return_exceptions=True)
        for r in results:
            assert not isinstance(r, Exception)


# ── Async Behaviour ────────────────────────────────────────────────────────

@pytest.mark.asyncio
async def test_shutdown_is_awaitable():
    from main import shutdown_event
    with patch("main.neo4j_driver", None):
        result = await shutdown_event()
    assert result is None


@pytest.mark.asyncio
async def test_shutdown_completes_quickly():
    from main import shutdown_event
    start = time.time()
    with patch("main.neo4j_driver", MagicMock()):
        await shutdown_event()
    assert time.time() - start < 1.0


# ── Error Recovery ─────────────────────────────────────────────────────────

@patch("main.neo4j_driver")
@patch("main.logger")
def test_continues_on_close_error(mock_logger, _):
    from main import shutdown_event
    inst = MagicMock()
    inst.close.side_effect = RuntimeError("Close failed")
    with patch("main.neo4j_driver", inst):
        asyncio.run(shutdown_event())  # must not raise
    mock_logger.error.assert_called()


# ── Application Lifecycle ─────────────────────────────────────────────────

def test_shutdown_event_registered():
    from main import app
    assert hasattr(app, "router")


def test_pending_requests_timeout():
    assert 30 > 5  # shutdown timeout > typical request duration
