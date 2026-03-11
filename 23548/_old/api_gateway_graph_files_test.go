package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"testing"
	"os"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
)

// setupTestRouter creates a router with the getGraphFiles route for testing
func setupTestRouter() *gin.Engine {
	gin.SetMode(gin.TestMode)
	router := gin.Default()
	router.GET("/api/graph/files", getGraphFiles)
	return router
}

// TestGetGraphFiles tests the API Gateway proxy for /api/graph/files
func TestGetGraphFiles(t *testing.T) {
	router := setupTestRouter()

	t.Run("Passed - Valid Query", func(t *testing.T) {
		// Mock the Graph Engine response using httptest.NewServer
		mockGraphEngine := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			assert.Equal(t, "/api/graph/files", r.URL.Path)
			assert.Equal(t, "550e8400-e29b-41d4-a716-446655440000", r.URL.Query().Get("repo_id"))
			
			w.WriteHeader(http.StatusOK)
			json.NewEncoder(w).Encode(map[string]interface{}{
				"nodes": []interface{}{},
				"edges": []interface{}{},
				"total_nodes": 0,
				"total_edges": 0,
			})
		}))
		defer mockGraphEngine.Close()

		// Temporarily override GRAPH_ENGINE_URL
		originalURL := os.Getenv("GRAPH_ENGINE_URL")
		os.Setenv("GRAPH_ENGINE_URL", mockGraphEngine.URL)
		defer os.Setenv("GRAPH_ENGINE_URL", originalURL)

		// Make the request to the API gateway
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/api/graph/files?repo_id=550e8400-e29b-41d4-a716-446655440000", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusOK, w.Code)
		
		var response map[string]interface{}
		err := json.Unmarshal(w.Body.Bytes(), &response)
		assert.NoError(t, err)
		assert.Contains(t, response, "nodes")
	})

	t.Run("Failed - Missing repo_id", func(t *testing.T) {
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/api/graph/files", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusBadRequest, w.Code)
		
		var response map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &response)
		assert.Equal(t, "Missing repo_id query parameter", response["error"])
	})

	t.Run("Failed - Graph Engine Error", func(t *testing.T) {
		// Mock Graph Engine to return an error
		mockGraphEngine := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusInternalServerError)
		}))
		defer mockGraphEngine.Close()

		os.Setenv("GRAPH_ENGINE_URL", mockGraphEngine.URL)

		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/api/graph/files?repo_id=550e8400-e29b-41d4-a716-446655440000", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusBadGateway, w.Code)
		
		var response map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &response)
		assert.Equal(t, "Graph engine returned an error", response["error"])
	})
}
