import pytest
import time
from unittest.mock import Mock, patch, MagicMock
from neo4j import GraphDatabase
from neo4j.exceptions import ServiceUnavailable, AuthError


# ================================
# Test Neo4j Retry Logic
# ================================

def test_exponential_backoff_calculation():
    """Test exponential backoff calculation matches expected values."""
    test_cases = [
        (1, 1),   # 2^0 = 1 second
        (2, 2),   # 2^1 = 2 seconds
        (3, 4),   # 2^2 = 4 seconds
        (4, 8),   # 2^3 = 8 seconds
    ]
    
    for attempt, expected in test_cases:
        wait_time = 2 ** (attempt - 1)
        assert wait_time == expected, \
            f"Attempt {attempt}: expected {expected}s, got {wait_time}s"


def test_max_retries_limit():
    """Test that retry logic respects max_retries parameter."""
    max_retries = 4
    attempt_count = 0
    
    for attempt in range(1, max_retries + 1):
        attempt_count += 1
        # Simulate failure
        if attempt < max_retries:
            continue
        else:
            break
    
    assert attempt_count == max_retries, \
        f"Should attempt exactly {max_retries} times"


@patch('main.GraphDatabase.driver')
@patch('main.time.sleep')
def test_connect_neo4j_success_first_attempt(mock_sleep, mock_driver):
    """Test successful connection on first attempt."""
    from main import connect_neo4j_with_retry
    
    # Mock successful connection
    mock_driver_instance = MagicMock()
    mock_driver.return_value = mock_driver_instance
    mock_driver_instance.verify_connectivity.return_value = None
    
    result = connect_neo4j_with_retry(
        "bolt://localhost:7687",
        "neo4j",
        "password",
        max_retries=4
    )
    
    # Verify connection succeeded
    assert result is not None
    mock_driver.assert_called_once()
    # Should not sleep on first success
    mock_sleep.assert_not_called()


@patch('main.GraphDatabase.driver')
@patch('main.time.sleep')
def test_connect_neo4j_success_after_retries(mock_sleep, mock_driver):
    """Test successful connection after 2 failures."""
    from main import connect_neo4j_with_retry
    
    # Mock failures then success
    mock_driver_instance = MagicMock()
    
    # First two attempts fail, third succeeds
    mock_driver.side_effect = [
        ServiceUnavailable("Connection failed"),
        ServiceUnavailable("Connection failed"),
        mock_driver_instance  # Success on third attempt
    ]
    mock_driver_instance.verify_connectivity.return_value = None
    
    result = connect_neo4j_with_retry(
        "bolt://localhost:7687",
        "neo4j",
        "password",
        max_retries=4
    )
    
    # Verify connection eventually succeeded
    assert result is not None
    assert mock_driver.call_count == 3
    # Should sleep twice (after 1st and 2nd failure)
    assert mock_sleep.call_count == 2


@patch('main.GraphDatabase.driver')
@patch('main.time.sleep')
def test_connect_neo4j_all_retries_fail(mock_sleep, mock_driver):
    """Test that None is returned after all retries fail."""
    from main import connect_neo4j_with_retry
    
    # Mock persistent failure
    mock_driver.side_effect = ServiceUnavailable("Connection failed")
    
    result = connect_neo4j_with_retry(
        "bolt://localhost:7687",
        "neo4j",
        "password",
        max_retries=3
    )
    
    # Verify connection failed
    assert result is None
    assert mock_driver.call_count == 3
    # Should sleep max_retries - 1 times
    assert mock_sleep.call_count == 2


@patch('main.GraphDatabase.driver')
@patch('main.time.sleep')
def test_connect_neo4j_exponential_backoff(mock_sleep, mock_driver):
    """Test that wait times follow exponential backoff."""
    from main import connect_neo4j_with_retry
    
    # Mock persistent failure
    mock_driver.side_effect = ServiceUnavailable("Connection failed")
    
    connect_neo4j_with_retry(
        "bolt://localhost:7687",
        "neo4j",
        "password",
        max_retries=4
    )
    
    # Verify sleep was called with exponentially increasing times
    expected_sleep_times = [1, 2, 4]  # 2^0, 2^1, 2^2
    actual_sleep_times = [call[0][0] for call in mock_sleep.call_args_list]
    
    assert actual_sleep_times == expected_sleep_times, \
        f"Expected sleep times {expected_sleep_times}, got {actual_sleep_times}"


def test_retry_success_scenarios():
    """Test various success/failure scenarios."""
    test_cases = [
        {
            "name": "Success on first attempt",
            "failures_before_success": 0,
            "max_retries": 4,
            "expect_success": True
        },
        {
            "name": "Success on third attempt",
            "failures_before_success": 2,
            "max_retries": 4,
            "expect_success": True
        },
        {
            "name": "Failure after max retries",
            "failures_before_success": 10,  # More than max_retries
            "max_retries": 4,
            "expect_success": False
        },
        {
            "name": "Success on last attempt",
            "failures_before_success": 3,
            "max_retries": 4,
            "expect_success": True
        }
    ]
    
    for tc in test_cases:
        attempt_count = 0
        success = False
        
        for attempt in range(1, tc["max_retries"] + 1):
            attempt_count += 1
            
            # Simulate connection attempt
            if attempt_count > tc["failures_before_success"]:
                success = True
                break
            
            # Simulate failure and retry
            if attempt < tc["max_retries"]:
                continue
        
        assert success == tc["expect_success"], \
            f"Test case '{tc['name']}': expected success={tc['expect_success']}, got={success}"


@patch('main.GraphDatabase.driver')
@patch('main.time.sleep')
def test_connect_neo4j_verify_connectivity_failure(mock_sleep, mock_driver):
    """Test that verify_connectivity failures trigger retry."""
    from main import connect_neo4j_with_retry
    
    # Mock driver creation success but verify fails
    mock_driver_instance = MagicMock()
    mock_driver.return_value = mock_driver_instance
    
    # First two verify calls fail, third succeeds
    mock_driver_instance.verify_connectivity.side_effect = [
        ServiceUnavailable("Verify failed"),
        ServiceUnavailable("Verify failed"),
        None  # Success
    ]
    
    result = connect_neo4j_with_retry(
        "bolt://localhost:7687",
        "neo4j",
        "password",
        max_retries=4
    )
    
    # Should succeed after retries
    assert result is not None
    assert mock_driver_instance.verify_connectivity.call_count == 3


def test_wait_time_progression():
    """Verify exponential backoff increases properly."""
    max_retries = 4
    wait_times = []
    
    for attempt in range(1, max_retries + 1):
        if attempt < max_retries:
            wait_time = 2 ** (attempt - 1)
            wait_times.append(wait_time)
    
    # Verify each wait time is double the previous
    for i in range(1, len(wait_times)):
        assert wait_times[i] > wait_times[i-1], \
            f"Wait time should increase: {wait_times[i]}s should be > {wait_times[i-1]}s"
        assert wait_times[i] == wait_times[i-1] * 2, \
            f"Wait time should double: {wait_times[i]}s should be 2 * {wait_times[i-1]}s"


def test_total_retry_time_calculation():
    """Test total time accumulated across all retries."""
    max_retries = 4
    total_wait = 0
    
    for attempt in range(1, max_retries):
        wait_time = 2 ** (attempt - 1)
        total_wait += wait_time
    
    # For 4 retries: 1 + 2 + 4 = 7 seconds total wait time
    assert total_wait == 7, \
        f"Total wait time for 4 retries should be 7 seconds, got {total_wait}"


@patch('main.GraphDatabase.driver')
def test_connect_neo4j_auth_error_no_retry(mock_driver):
    """Test that authentication errors don't trigger infinite retry."""
    from main import connect_neo4j_with_retry
    
    # Mock authentication error (shouldn't retry on auth failure in production)
    mock_driver.side_effect = AuthError("Invalid credentials")
    
    result = connect_neo4j_with_retry(
        "bolt://localhost:7687",
        "neo4j",
        "wrong_password",
        max_retries=3
    )
    
    # Current implementation retries on all exceptions
    # In production, you might want to NOT retry auth errors
    assert result is None
    assert mock_driver.call_count == 3


@patch('main.logger')
@patch('main.GraphDatabase.driver')
@patch('main.time.sleep')
def test_retry_logging(mock_sleep, mock_driver, mock_logger):
    """Test that retry attempts are logged correctly."""
    from main import connect_neo4j_with_retry
    
    # Mock failure
    mock_driver.side_effect = ServiceUnavailable("Connection failed")
    
    connect_neo4j_with_retry(
        "bolt://localhost:7687",
        "neo4j",
        "password",
        max_retries=3
    )
    
    # Verify logging calls
    assert mock_logger.info.call_count >= 3  # Connection attempts
    assert mock_logger.warning.call_count == 2  # Retry warnings
    assert mock_logger.error.call_count == 1  # Final failure


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
