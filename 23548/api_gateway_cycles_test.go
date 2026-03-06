package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
)

// setupRouterForDetectCycles creates a minimal router with the endpoint we need
func setupRouterForDetectCycles() *gin.Engine {
	gin.SetMode(gin.TestMode)
	router := gin.Default()
	router.POST("/api/graph/:repo_id/detect-cycles", detectCycles)
	return router
}

// TestDetectCycles proxy logic tests
func TestDetectCycles(t *testing.T) {
	router := setupRouterForDetectCycles()

	t.Run("Passed - Valid Query", func(t *testing.T) {
		// Mock upstream Graph Engine response
		mockGraphEngine := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			assert.Equal(t, "/api/graph/550e8400-e29b-41d4-a716-446655440000/detect-cycles", r.URL.Path)
			assert.Equal(t, "POST", r.Method)
			
			w.WriteHeader(http.StatusOK)
			json.NewEncoder(w).Encode(map[string]interface{}{
				"message":            "Cycle detection completed successfully",
				"total_cycles_found": 1,
				"nodes_flagged":      3,
				"edges_flagged":      3,
			})
		}))
		defer mockGraphEngine.Close()

		// Temporarily override GRAPH_ENGINE_URL
		originalURL := os.Getenv("GRAPH_ENGINE_URL")
		os.Setenv("GRAPH_ENGINE_URL", mockGraphEngine.URL)
		defer os.Setenv("GRAPH_ENGINE_URL", originalURL)

		// Make request to API gateway proxy
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("POST", "/api/graph/550e8400-e29b-41d4-a716-446655440000/detect-cycles", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusOK, w.Code)
		
		var response map[string]interface{}
		err := json.Unmarshal(w.Body.Bytes(), &response)
		assert.NoError(t, err)
		assert.Equal(t, float64(1), response["total_cycles_found"])
	})

	t.Run("Failed - Invalid Repo ID", func(t *testing.T) {
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("POST", "/api/graph/invalid_id_not_uuid/detect-cycles", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusBadRequest, w.Code)
		
		var response map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &response)
		assert.Equal(t, "Invalid repo_id format", response["error"])
	})

	t.Run("Failed - Graph Engine Gateway Error", func(t *testing.T) {
		// Mock upstream Graph Engine to fail
		mockGraphEngine := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusInternalServerError)
		}))
		defer mockGraphEngine.Close()

		os.Setenv("GRAPH_ENGINE_URL", mockGraphEngine.URL)

		w := httptest.NewRecorder()
		req, _ := http.NewRequest("POST", "/api/graph/550e8400-e29b-41d4-a716-446655440000/detect-cycles", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusBadGateway, w.Code)
		
		var response map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &response)
		assert.Equal(t, "Graph engine returned an error", response["error"])
	})
}
