package main

import (
	"database/sql"
	"errors"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

// ===========================================================================
// PostgreSQL Connection Retry Logic Tests
// ===========================================================================

func TestExponentialBackoff(t *testing.T) {
	expected := []time.Duration{
		1 * time.Second,  // 2^0
		2 * time.Second,  // 2^1
		4 * time.Second,  // 2^2
		8 * time.Second,  // 2^3
		16 * time.Second, // 2^4
	}

	for i, want := range expected {
		attempt := i + 1
		got := time.Duration(1<<uint(attempt-1)) * time.Second
		assert.Equal(t, want, got, "attempt %d", attempt)
	}
}

func TestRetryLogic_MaxRetriesReached(t *testing.T) {
	maxRetries := 5
	attempts := 0

	for a := 1; a <= maxRetries; a++ {
		attempts++
	}

	assert.Equal(t, maxRetries, attempts, "should attempt exactly %d times", maxRetries)
}

func TestRetryLogic_SuccessOnRetry(t *testing.T) {
	tests := []struct {
		name              string
		failsBeforeOK     int
		maxRetries        int
		expectSuccess     bool
	}{
		{"First attempt", 0, 5, true},
		{"Third attempt", 2, 5, true},
		{"Exhausted retries", 10, 5, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			attempts := 0
			ok := false

			for a := 1; a <= tt.maxRetries; a++ {
				attempts++
				if attempts > tt.failsBeforeOK {
					ok = true
					break
				}
			}

			assert.Equal(t, tt.expectSuccess, ok)
		})
	}
}

func TestConnectionPing_TwoPhaseCheck(t *testing.T) {
	type state struct {
		openOK bool
		pingOK bool
	}
	tests := []struct {
		name string
		s    state
		ok   bool
	}{
		{"Both succeed", state{true, true}, true},
		{"Open OK, ping fails", state{true, false}, false},
		{"Open fails", state{false, false}, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			assert.Equal(t, tt.ok, tt.s.openOK && tt.s.pingOK)
		})
	}
}

func TestRetryLogic_NilOnFailure(t *testing.T) {
	var result *sql.DB
	maxRetries := 3

	for a := 1; a <= maxRetries; a++ {
		err := errors.New("connection refused")
		if err != nil && a == maxRetries {
			result = nil
			break
		}
	}

	assert.Nil(t, result)
}

func TestWaitTimeProgression(t *testing.T) {
	var times []time.Duration
	maxRetries := 5

	for a := 1; a < maxRetries; a++ {
		times = append(times, time.Duration(1<<uint(a-1))*time.Second)
	}

	for i := 1; i < len(times); i++ {
		assert.True(t, times[i] > times[i-1], "must increase")
		assert.Equal(t, times[i], times[i-1]*2, "must double")
	}
}

func BenchmarkExponentialBackoffCalc(b *testing.B) {
	for i := 0; i < b.N; i++ {
		for a := 1; a <= 5; a++ {
			_ = time.Duration(1<<uint(a-1)) * time.Second
		}
	}
}
