package main

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

func TestCircuitBreaker_StateTransitions(t *testing.T) {
	cb := &CircuitBreaker{
		State:       Closed,
		MaxFailures: 3,
		Timeout:     50 * time.Millisecond,
	}

	// Initial state should allow requests
	assert.True(t, cb.AllowRequest(), "Closed state should allow requests")

	// Record successes shouldn't change state
	cb.RecordSuccess()
	assert.Equal(t, Closed, cb.State)
	assert.Equal(t, 0, cb.Failures)

	// Record failures
	cb.RecordFailure()
	assert.Equal(t, Closed, cb.State)
	assert.Equal(t, 1, cb.Failures)

	cb.RecordFailure()
	assert.Equal(t, 2, cb.Failures)

	cb.RecordFailure()
	assert.Equal(t, Open, cb.State, "Circuit should trip open after 3 failures")
	assert.Equal(t, 3, cb.Failures)

	// Open state should reject requests
	assert.False(t, cb.AllowRequest(), "Open circuit should reject requests")

	// Wait for timeout to transition to HalfOpen
	time.Sleep(60 * time.Millisecond)

	// Should allow one request as HalfOpen
	assert.True(t, cb.AllowRequest(), "Should allow trial request after timeout")
	assert.Equal(t, HalfOpen, cb.State, "State should be HalfOpen")

	// Should reject subsequent requests while in HalfOpen
	assert.False(t, cb.AllowRequest(), "Should reject extra requests in HalfOpen")

	// If the HalfOpen request succeeds, it should close
	cb.RecordSuccess()
	assert.Equal(t, Closed, cb.State, "Should close after successful HalfOpen request")
	assert.Equal(t, 0, cb.Failures)

	// If it fails again and gets to HalfOpen
	cb.RecordFailure()
	cb.RecordFailure()
	cb.RecordFailure()
	assert.Equal(t, Open, cb.State)
	
	time.Sleep(60 * time.Millisecond)
	assert.True(t, cb.AllowRequest()) // transitions to HalfOpen

	// Record failure drops it back to Open
	cb.RecordFailure()
	assert.Equal(t, Open, cb.State, "Should return to Open if HalfOpen request fails")
}
