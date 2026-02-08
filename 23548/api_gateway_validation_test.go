package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
)

// ==========================================
// Unit Tests for Validation Functions
// ==========================================

func TestValidateRepoURL(t *testing.T) {
	tests := []struct {
		name     string
		url      string
		expected bool
	}{
		// Valid URLs
		{"Valid HTTPS", "https://github.com/user/repo", true},
		{"Valid HTTPS .git", "https://github.com/user/repo.git", true},
		{"Valid HTTP", "http://gitlab.com/group/project.git", true},
		{"Valid SSH", "git@github.com:user/repo.git", true},
		{"Valid SSH Custom Domain", "ssh://user@server.com/project.git", true},
		{"Valid Subdirectory", "https://github.com/user/repo/tree/main", true},

		// Invalid URLs
		{"Empty String", "", false},
		{"No Protocol", "github.com/user/repo", false},
		{"Just Protocol", "https://", false},
		{"Invalid Characters", "https://github.com/user/repo with spaces", false},
		{"SQL Injection Attempt", "https://github.com/user/repo'; DROP TABLE users; --", false},
		{"Command Injection Attempt", "https://github.com/user/repo && rm -rf /", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := validateRepoURL(tt.url)
			assert.Equal(t, tt.expected, result, "URL: %s", tt.url)
		})
	}
}

func TestValidateBranchName(t *testing.T) {
	tests := []struct {
		name     string
		branch   string
		expected bool
	}{
		// Valid Branch Names
		{"Standard Main", "main", true},
		{"Standard Master", "master", true},
		{"Feature Branch", "feature/login-page", true},
		{"Bugfix Branch", "bugfix/issue-123", true},
		{"Release Branch", "release-v1.0.0", true},
		{"With Dots", "v1.2.3", true},
		{"With Underscores", "my_branch_name", true},

		// Invalid Branch Names
		{"Empty String", "", false},
		{"Directory Traversal", "../../../etc/passwd", false},
		{"Starts with Slash", "/dev/null", false}, // Our regex might allow this, let's see implementation
		{"Space", "feature branch", false},
		{"Control Characters", "branch\nname", false},
		{"Wildcard", "feature/*", false},
		{"Tilde", "branch~1", false},
		{"Carrot", "branch^", false},
		{"Colon", "branch:name", false},
		{"Question Mark", "branch?", false},
		{"Backslash", "branch\\name", false},
	}

	// Note: Current implementation allows slashes anywhere, but disallows ".."
	// regex: ^[a-zA-Z0-9_\-\./]+$

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := validateBranchName(tt.branch)
			assert.Equal(t, tt.expected, result, "Branch: %s", tt.branch)
		})
	}
}

func TestValidateUUID(t *testing.T) {
	tests := []struct {
		name     string
		uuid     string
		expected bool
	}{
		{"Valid UUID v4", "550e8400-e29b-41d4-a716-446655440000", true},
		{"Valid UUID v1", "6ba7b810-9dad-11d1-80b4-00c04fd430c8", true},
		{"Uppercase UUID", "550E8400-E29B-41D4-A716-446655440000", true},

		{"Invalid Length", "550e8400-e29b-41d4-a716-4466554400", false},
		{"Invalid Characters", "550e8400-e29b-41d4-a716-ZZZZZZZZZZZZ", false},
		{"Empty String", "", false},
		{"Garbage", "not-a-uuid", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := validateUUID(tt.uuid)
			assert.Equal(t, tt.expected, result, "UUID: %s", tt.uuid)
		})
	}
}

// ==========================================
// Integration Tests for Handlers
// ==========================================

func TestAnalyzeRepository_Validation(t *testing.T) {
	// Setup Gin
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.POST("/analyze", analyzeRepository)

	tests := []struct {
		name         string
		payload      string
		expectedCode int
		expectedMsg  string
	}{
		{
			name:         "Missing Repo URL",
			payload:      `{"branch": "main"}`,
			expectedCode: 400,
			expectedMsg:  "Invalid request body", // Binding fails
		},
		{
			name:         "Invalid Repo URL",
			payload:      `{"repo_url": "not-a-url", "branch": "main"}`,
			expectedCode: 400,
			expectedMsg:  "Invalid git repository URL format",
		},
		{
			name:         "Invalid Branch Name",
			payload:      `{"repo_url": "https://github.com/user/repo", "branch": "bad branch"}`,
			expectedCode: 400,
			expectedMsg:  "Invalid branch name format",
		},
		{
			name:         "Dangerous Branch Name",
			payload:      `{"repo_url": "https://github.com/user/repo", "branch": "../../../etc/passwd"}`,
			expectedCode: 400,
			expectedMsg:  "Invalid branch name format",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			w := httptest.NewRecorder()
			req, _ := http.NewRequest("POST", "/analyze", strings.NewReader(tt.payload))
			req.Header.Set("Content-Type", "application/json")

			router.ServeHTTP(w, req)

			assert.Equal(t, tt.expectedCode, w.Code)

			var response map[string]interface{}
			err := json.Unmarshal(w.Body.Bytes(), &response)
			assert.NoError(t, err)

			// Check error message logic
			if tt.expectedCode == 400 {
				// Depending on how validationError maps response
				// It sets "error" or "message"
				found := false
				if msg, ok := response["message"]; ok && strings.Contains(msg.(string), tt.expectedMsg) {
					found = true
				}
				if msg, ok := response["error"]; ok && strings.Contains(msg.(string), tt.expectedMsg) {
					found = true
				}
				// Also check details
				if msg, ok := response["details"]; ok && strings.Contains(msg.(string), tt.expectedMsg) {
					found = true
				}

				assert.True(t, found, "Response should contain error message: %s. Got: %v", tt.expectedMsg, response)
			}
		})
	}
}

func TestGetJobStatus_Validation(t *testing.T) {
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.GET("/jobs/:id", getJobStatus)

	w := httptest.NewRecorder()
	req, _ := http.NewRequest("GET", "/jobs/invalid-uuid", nil)

	router.ServeHTTP(w, req)

	assert.Equal(t, 400, w.Code)

	var response map[string]interface{}
	json.Unmarshal(w.Body.Bytes(), &response)

	assert.Equal(t, "Validation Error", response["error"])
	assert.Equal(t, "id", response["field"])
	assert.Contains(t, response["message"], "Invalid UUID format")
}

func TestUpdateJob_Validation(t *testing.T) {
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.PATCH("/jobs/:id", updateJob)

	tests := []struct {
		name         string
		jobID        string
		payload      string
		expectedCode int
		expectedMsg  string
	}{
		{
			name:         "Invalid UUID",
			jobID:        "bad-id",
			payload:      `{"status": "COMPLETED"}`,
			expectedCode: 400,
			expectedMsg:  "Invalid UUID format",
		},
		{
			name:         "Invalid Progress Range High",
			jobID:        "550e8400-e29b-41d4-a716-446655440000",
			payload:      `{"progress": 150}`,
			expectedCode: 400,
			expectedMsg:  "Progress must be between 0 and 100",
		},
		{
			name:         "Invalid Progress Range Low",
			jobID:        "550e8400-e29b-41d4-a716-446655440000",
			payload:      `{"progress": -5}`,
			expectedCode: 400,
			expectedMsg:  "Progress must be between 0 and 100",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			w := httptest.NewRecorder()
			req, _ := http.NewRequest("PATCH", "/jobs/"+tt.jobID, strings.NewReader(tt.payload))
			req.Header.Set("Content-Type", "application/json")

			router.ServeHTTP(w, req)

			assert.Equal(t, tt.expectedCode, w.Code)

			var response map[string]interface{}
			json.Unmarshal(w.Body.Bytes(), &response)

			// Check for error message in any field
			found := false
			for _, v := range response {
				if str, ok := v.(string); ok && strings.Contains(str, tt.expectedMsg) {
					found = true
					break
				}
			}
			assert.True(t, found, "Response should contain: %s", tt.expectedMsg)
		})
	}
}
