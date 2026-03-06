package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
)

// setupRouterForFunctionFlow creates a router mapping strictly for function flow testing
func setupRouterForFunctionFlow() *gin.Engine {
	gin.SetMode(gin.TestMode)
	router := gin.Default()
	router.GET("/api/graph/functions/:id/flow", getFunctionFlow)
	return router
}

// TestGetFunctionFlow tests the API Gateway proxy for /api/graph/functions/:id/flow
func TestGetFunctionFlow(t *testing.T) {
	router := setupRouterForFunctionFlow()

	t.Run("Passed - Valid Query", func(t *testing.T) {
		// Mock upstream Graph Engine response
		mockGraphEngine := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			assert.Equal(t, "/api/graph/functions/func_test_123/flow", r.URL.Path)
			assert.Equal(t, "3", r.URL.Query().Get("depth"))
			
			w.WriteHeader(http.StatusOK)
			json.NewEncoder(w).Encode(map[string]interface{}{
				"nodes":       []interface{}{},
				"edges":       []interface{}{},
				"total_nodes": 0,
				"total_edges": 0,
			})
		}))
		defer mockGraphEngine.Close()

		// Temporarily override GRAPH_ENGINE_URL
		originalURL := os.Getenv("GRAPH_ENGINE_URL")
		os.Setenv("GRAPH_ENGINE_URL", mockGraphEngine.URL)
		defer os.Setenv("GRAPH_ENGINE_URL", originalURL)

		// Make request to API gateway proxy
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/api/graph/functions/func_test_123/flow?depth=3", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusOK, w.Code)
		
		var response map[string]interface{}
		err := json.Unmarshal(w.Body.Bytes(), &response)
		assert.NoError(t, err)
		assert.Contains(t, response, "nodes")
	})

	t.Run("Failed - Graph Engine Gateway Error", func(t *testing.T) {
		// Mock upstream Graph Engine to fail
		mockGraphEngine := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusInternalServerError)
		}))
		defer mockGraphEngine.Close()

		os.Setenv("GRAPH_ENGINE_URL", mockGraphEngine.URL)

		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/api/graph/functions/func_test_123/flow", nil)
		router.ServeHTTP(w, req)

		assert.Equal(t, http.StatusBadGateway, w.Code)
		
		var response map[string]interface{}
		json.Unmarshal(w.Body.Bytes(), &response)
		assert.Equal(t, "Graph engine returned an error", response["error"])
	})
}
