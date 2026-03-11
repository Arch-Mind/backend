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

// ===========================================================================
// Detect Cycles Proxy Tests – API Gateway → Graph Engine
// ===========================================================================

func setupCyclesRouter() *gin.Engine {
	gin.SetMode(gin.TestMode)
	r := gin.Default()
	r.POST("/api/graph/:repo_id/detect-cycles", detectCycles)
	return r
}

func TestDetectCycles(t *testing.T) {
	router := setupCyclesRouter()

	t.Run("Valid request proxies to graph engine", func(t *testing.T) {
		mock := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
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
		defer mock.Close()

		orig := os.Getenv("GRAPH_ENGINE_URL")
		os.Setenv("GRAPH_ENGINE_URL", mock.URL)
		defer os.Setenv("GRAPH_ENGINE_URL", orig)

		w := httptest.NewRecorder()
		req, _ := http.NewRequest("POST", "/api/graph/550e8400-e29b-41d4-a716-446655440000/detect-cycles", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusOK, w.Code)
		var resp map[string]interface{}
		assert.NoError(t, json.Unmarshal(w.Body.Bytes(), &resp))
		assert.Equal(t, float64(1), resp["total_cycles_found"])
	})

	t.Run("Invalid repo_id returns 400", func(t *testing.T) {
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("POST", "/api/graph/invalid_id/detect-cycles", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusBadRequest, w.Code)
		var resp map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &resp)
		assert.Equal(t, "Invalid repo_id format", resp["error"])
	})

	t.Run("Graph engine error returns 502", func(t *testing.T) {
		mock := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusInternalServerError)
		}))
		defer mock.Close()
		os.Setenv("GRAPH_ENGINE_URL", mock.URL)

		w := httptest.NewRecorder()
		req, _ := http.NewRequest("POST", "/api/graph/550e8400-e29b-41d4-a716-446655440000/detect-cycles", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusBadGateway, w.Code)
		var resp map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &resp)
		assert.Equal(t, "Graph engine returned an error", resp["error"])
	})
}
