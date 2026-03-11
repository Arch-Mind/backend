package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
)

// ===========================================================================
// Circuit Breaker Tests - API Gateway -> Graph Engine
// ===========================================================================

func TestCircuitBreaker_StateTransitions(t *testing.T) {
	cb := &CircuitBreaker{
		State:       Closed,
		MaxFailures: 3,
		Timeout:     50 * time.Millisecond,
	}

	// Closed: requests pass through
	assert.True(t, cb.AllowRequest())

	// Successes keep it closed
	cb.RecordSuccess()
	assert.Equal(t, Closed, cb.State)
	assert.Equal(t, 0, cb.FailureCount)

	// Accumulate failures up to threshold
	cb.RecordFailure()
	assert.Equal(t, 1, cb.FailureCount)
	cb.RecordFailure()
	assert.Equal(t, 2, cb.FailureCount)
	cb.RecordFailure()
	assert.Equal(t, Open, cb.State, "should trip open after MaxFailures")

	// Open: requests rejected
	assert.False(t, cb.AllowRequest())

	// After timeout -> HalfOpen allows trial requests
	time.Sleep(60 * time.Millisecond)
	assert.True(t, cb.AllowRequest())
	assert.Equal(t, HalfOpen, cb.State)
	// HalfOpen allows requests through (implementation returns true)
	assert.True(t, cb.AllowRequest(), "HalfOpen allows requests through")

	// Trial success -> back to Closed
	cb.RecordSuccess()
	assert.Equal(t, Closed, cb.State)
	assert.Equal(t, 0, cb.FailureCount)

	// Trip again, then HalfOpen trial failure -> back to Open
	cb.RecordFailure()
	cb.RecordFailure()
	cb.RecordFailure()
	assert.Equal(t, Open, cb.State)
	time.Sleep(60 * time.Millisecond)
	cb.AllowRequest() // -> HalfOpen
	cb.RecordFailure()
	assert.Equal(t, Open, cb.State, "HalfOpen failure returns to Open")
}

func setupGraphProxyRouter() *gin.Engine {
	gin.SetMode(gin.TestMode)
	r := gin.New()
	r.Use(gin.Recovery())
	r.GET("/api/graph/files", getGraphFiles)
	return r
}

func TestGraphProxy_CircuitBreakerOpen(t *testing.T) {
	router := setupGraphProxyRouter()

	// Force circuit breaker to Open state
	graphBreaker.State = Open
	graphBreaker.FailureCount = 3
	graphBreaker.LastFailure = time.Now()

	w := httptest.NewRecorder()
	req, _ := http.NewRequest("GET", "/api/graph/files?repo_id=test", nil)
	router.ServeHTTP(w, req)

	assert.Equal(t, http.StatusServiceUnavailable, w.Code)

	var resp map[string]interface{}
	json.Unmarshal(w.Body.Bytes(), &resp)
	assert.Contains(t, resp["error"], "Circuit breaker open")

	// Reset for other tests
	graphBreaker.State = Closed
	graphBreaker.FailureCount = 0
}

func TestCircuitBreaker_TripsOnConsecutiveFailures(t *testing.T) {
	router := setupGraphProxyRouter()

	mock := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer mock.Close()
	os.Setenv("GRAPH_ENGINE_URL", mock.URL)

	graphBreaker.State = Closed
	graphBreaker.FailureCount = 0

	// 3 failures -> Open
	for i := 0; i < 3; i++ {
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/api/graph/files?repo_id=test", nil)
		router.ServeHTTP(w, req)
		assert.Equal(t, http.StatusBadGateway, w.Code)
	}

	// 4th request -> 503 Circuit Breaker Open
	w := httptest.NewRecorder()
	req, _ := http.NewRequest("GET", "/api/graph/files?repo_id=test", nil)
	router.ServeHTTP(w, req)
	assert.Equal(t, http.StatusServiceUnavailable, w.Code)

	var resp map[string]interface{}
	json.Unmarshal(w.Body.Bytes(), &resp)
	assert.Contains(t, resp["error"], "Circuit breaker open")

	// Reset for other tests
	graphBreaker.State = Closed
	graphBreaker.FailureCount = 0
}
