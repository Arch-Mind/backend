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

func setupRouterForErrorHandling() *gin.Engine {
	gin.SetMode(gin.TestMode)
	router := gin.Default()
	router.GET("/health", healthCheck)
	router.GET("/api/graph/files", getGraphFiles)
	return router
}

func TestHealthCheckAndCircuitBreaker(t *testing.T) {
	router := setupRouterForErrorHandling()

	t.Run("Health Check - Upstream Healthy", func(t *testing.T) {
		mockGraphEngine := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusOK)
		}))
		defer mockGraphEngine.Close()

		os.Setenv("GRAPH_ENGINE_URL", mockGraphEngine.URL)

		// Set State to Closed
		graphBreaker.State = Closed
		graphBreaker.FailureCount = 0

		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/health", nil)
		router.ServeHTTP(w, req)

		var response map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &response)
		
		details := response["details"].(map[string]interface{})
		assert.Equal(t, "UP", details["graph_engine"])
	})

	t.Run("Circuit Breaker - Trips on Consecutive Failures", func(t *testing.T) {
		mockGraphEngine := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusInternalServerError)
		}))
		defer mockGraphEngine.Close()

		os.Setenv("GRAPH_ENGINE_URL", mockGraphEngine.URL)
        
        // Reset state
        graphBreaker.State = Closed
        graphBreaker.FailureCount = 0

		// Fail 3 times, which is our MaxFailures default
		for i := 0; i < 3; i++ {
			w := httptest.NewRecorder()
			req, _ := http.NewRequest("GET", "/api/graph/files?repo_id=test", nil)
			router.ServeHTTP(w, req)
			assert.Equal(t, http.StatusBadGateway, w.Code)
		}

		// 4th time should be 503 Circuit Breaker Open immediately
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/api/graph/files?repo_id=test", nil)
		router.ServeHTTP(w, req)
		assert.Equal(t, http.StatusServiceUnavailable, w.Code)

		var response map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &response)
		assert.Contains(t, response["error"], "Circuit breaker open")
	})
}
