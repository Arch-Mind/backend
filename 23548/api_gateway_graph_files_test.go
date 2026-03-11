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
// Graph Files Proxy Tests – API Gateway → Graph Engine
// ===========================================================================

func setupGraphFilesRouter() *gin.Engine {
	gin.SetMode(gin.TestMode)
	r := gin.Default()
	r.GET("/api/graph/files", getGraphFiles)
	return r
}

func TestGetGraphFiles(t *testing.T) {
	router := setupGraphFilesRouter()

	t.Run("Valid query proxies to graph engine", func(t *testing.T) {
		mock := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			assert.Equal(t, "/api/graph/files", r.URL.Path)
			assert.Equal(t, "550e8400-e29b-41d4-a716-446655440000", r.URL.Query().Get("repo_id"))
			w.WriteHeader(http.StatusOK)
			json.NewEncoder(w).Encode(map[string]interface{}{
				"nodes": []interface{}{}, "edges": []interface{}{},
				"total_nodes": 0, "total_edges": 0,
			})
		}))
		defer mock.Close()

		orig := os.Getenv("GRAPH_ENGINE_URL")
		os.Setenv("GRAPH_ENGINE_URL", mock.URL)
		defer os.Setenv("GRAPH_ENGINE_URL", orig)

		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/api/graph/files?repo_id=550e8400-e29b-41d4-a716-446655440000", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusOK, w.Code)
		var resp map[string]interface{}
		assert.NoError(t, json.Unmarshal(w.Body.Bytes(), &resp))
		assert.Contains(t, resp, "nodes")
	})

	t.Run("Missing repo_id returns 400", func(t *testing.T) {
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/api/graph/files", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusBadRequest, w.Code)
		var resp map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &resp)
		assert.Equal(t, "Missing repo_id query parameter", resp["error"])
	})

	t.Run("Graph engine error returns 502", func(t *testing.T) {
		mock := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusInternalServerError)
		}))
		defer mock.Close()
		os.Setenv("GRAPH_ENGINE_URL", mock.URL)

		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/api/graph/files?repo_id=550e8400-e29b-41d4-a716-446655440000", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusBadGateway, w.Code)
		var resp map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &resp)
		assert.Equal(t, "Graph engine returned an error", resp["error"])
	})
}
