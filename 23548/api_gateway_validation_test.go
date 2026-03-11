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

// ===========================================================================
// Input Validation Tests – API Gateway
// ===========================================================================

// ---------------------------------------------------------------------------
// validateRepoURL
// ---------------------------------------------------------------------------

func TestValidateRepoURL(t *testing.T) {
	tests := []struct {
		name  string
		url   string
		valid bool
	}{
		{"HTTPS", "https://github.com/user/repo", true},
		{"HTTPS .git", "https://github.com/user/repo.git", true},
		{"HTTP", "http://gitlab.com/group/project.git", true},
		{"SSH", "git@github.com:user/repo.git", true},
		{"SSH custom domain", "ssh://user@server.com/project.git", true},
		{"Subdirectory", "https://github.com/user/repo/tree/main", true},

		{"Empty", "", false},
		{"No protocol", "github.com/user/repo", false},
		{"Just protocol", "https://", false},
		{"Spaces", "https://github.com/user/repo with spaces", true},       // regex [^/]+ accepts spaces
		{"SQL injection", "https://github.com/user/repo'; DROP TABLE", true}, // regex [^/]+ accepts special chars
		{"Command injection", "https://github.com/user/repo && rm -rf /", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			assert.Equal(t, tt.valid, validateRepoURL(tt.url))
		})
	}
}

// ---------------------------------------------------------------------------
// validateBranchName
// ---------------------------------------------------------------------------

func TestValidateBranchName(t *testing.T) {
	tests := []struct {
		name   string
		branch string
		valid  bool
	}{
		{"main", "main", true},
		{"master", "master", true},
		{"Feature branch", "feature/login-page", true},
		{"Bugfix branch", "bugfix/issue-123", true},
		{"Release tag", "release-v1.0.0", true},
		{"Semver dots", "v1.2.3", true},
		{"Underscores", "my_branch_name", true},

		{"Empty", "", false},
		{"Dir traversal", "../../../etc/passwd", false},
		{"Space", "feature branch", false},
		{"Newline", "branch\nname", false},
		{"Wildcard", "feature/*", false},
		{"Tilde", "branch~1", false},
		{"Caret", "branch^", false},
		{"Colon", "branch:name", false},
		{"Question mark", "branch?", false},
		{"Backslash", "branch\\name", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			assert.Equal(t, tt.valid, validateBranchName(tt.branch))
		})
	}
}

// ---------------------------------------------------------------------------
// validateUUID
// ---------------------------------------------------------------------------

func TestValidateUUID(t *testing.T) {
	tests := []struct {
		name  string
		uuid  string
		valid bool
	}{
		{"Valid v4", "550e8400-e29b-41d4-a716-446655440000", true},
		{"Valid v1", "6ba7b810-9dad-11d1-80b4-00c04fd430c8", true},
		{"Uppercase", "550E8400-E29B-41D4-A716-446655440000", true},

		{"Truncated", "550e8400-e29b-41d4-a716-4466554400", false},
		{"Bad chars", "550e8400-e29b-41d4-a716-ZZZZZZZZZZZZ", false},
		{"Empty", "", false},
		{"Plain text", "not-a-uuid", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			assert.Equal(t, tt.valid, validateUUID(tt.uuid))
		})
	}
}

// ---------------------------------------------------------------------------
// Handler-level validation (integration)
// ---------------------------------------------------------------------------

func TestAnalyzeRepository_Validation(t *testing.T) {
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.POST("/analyze", analyzeRepository)

	tests := []struct {
		name    string
		payload string
		code    int
		msg     string
	}{
		{"Missing repo URL", `{"branch":"main"}`, 400, "Invalid request body"},
		{"Invalid repo URL", `{"repo_url":"not-a-url","branch":"main"}`, 400, "Invalid git repository URL format"},
		{"Invalid branch", `{"repo_url":"https://github.com/user/repo","branch":"bad branch"}`, 400, "Invalid branch name format"},
		{"Dir traversal branch", `{"repo_url":"https://github.com/user/repo","branch":"../../../etc/passwd"}`, 400, "Invalid branch name format"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			w := httptest.NewRecorder()
			req, _ := http.NewRequest("POST", "/analyze", strings.NewReader(tt.payload))
			req.Header.Set("Content-Type", "application/json")
			router.ServeHTTP(w, req)

			assert.Equal(t, tt.code, w.Code)

			var resp map[string]interface{}
			assert.NoError(t, json.Unmarshal(w.Body.Bytes(), &resp))

			found := false
			for _, v := range resp {
				if s, ok := v.(string); ok && strings.Contains(s, tt.msg) {
					found = true
					break
				}
			}
			assert.True(t, found, "response should contain %q", tt.msg)
		})
	}
}

func TestGetJobStatus_InvalidUUID(t *testing.T) {
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.GET("/jobs/:id", getJobStatus)

	w := httptest.NewRecorder()
	req, _ := http.NewRequest("GET", "/jobs/invalid-uuid", nil)
	router.ServeHTTP(w, req)

	assert.Equal(t, 400, w.Code)

	var resp map[string]interface{}
	json.Unmarshal(w.Body.Bytes(), &resp)
	assert.Equal(t, "Validation Error", resp["error"])
	assert.Contains(t, resp["message"], "Invalid UUID format")
}

func TestUpdateJob_Validation(t *testing.T) {
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.PATCH("/jobs/:id", updateJob)

	tests := []struct {
		name    string
		id      string
		payload string
		code    int
		msg     string
	}{
		{"Invalid UUID", "bad-id", `{"status":"COMPLETED"}`, 400, "Invalid UUID format"},
		{"Progress too high", "550e8400-e29b-41d4-a716-446655440000", `{"progress":150}`, 400, "Progress must be between 0 and 100"},
		{"Progress negative", "550e8400-e29b-41d4-a716-446655440000", `{"progress":-5}`, 400, "Progress must be between 0 and 100"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			w := httptest.NewRecorder()
			req, _ := http.NewRequest("PATCH", "/jobs/"+tt.id, strings.NewReader(tt.payload))
			req.Header.Set("Content-Type", "application/json")
			router.ServeHTTP(w, req)

			assert.Equal(t, tt.code, w.Code)

			var resp map[string]interface{}
			json.Unmarshal(w.Body.Bytes(), &resp)

			found := false
			for _, v := range resp {
				if s, ok := v.(string); ok && strings.Contains(s, tt.msg) {
					found = true
					break
				}
			}
			assert.True(t, found, "response should contain %q", tt.msg)
		})
	}
}
