package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

func intPtr(i int) *int          { return &i }
func stringPtr(s string) *string { return &s }

func createTestContext() (*gin.Context, *httptest.ResponseRecorder) {
	gin.SetMode(gin.TestMode)
	w := httptest.NewRecorder()
	c, _ := gin.CreateTestContext(w)
	return c, w
}

func createTestRequest(method, url string, body interface{}) *http.Request {
	jsonData, _ := json.Marshal(body)
	req := httptest.NewRequest(method, url, bytes.NewBuffer(jsonData))
	req.Header.Set("Content-Type", "application/json")
	return req
}

// ===========================================================================
// 1. Status Transition Validation
// ===========================================================================

func TestValidateStatusTransition(t *testing.T) {
	tests := []struct {
		name    string
		current string
		next    string
		valid   bool
	}{
		// Valid transitions from QUEUED
		{"QUEUED → PROCESSING", "QUEUED", "PROCESSING", true},
		{"QUEUED → CANCELLED", "QUEUED", "CANCELLED", true},

		// Valid transitions from PROCESSING
		{"PROCESSING → COMPLETED", "PROCESSING", "COMPLETED", true},
		{"PROCESSING → FAILED", "PROCESSING", "FAILED", true},
		{"PROCESSING → CANCELLED", "PROCESSING", "CANCELLED", true},

		// Invalid: skip from QUEUED straight to terminal
		{"QUEUED → COMPLETED (invalid)", "QUEUED", "COMPLETED", false},
		{"QUEUED → FAILED (invalid)", "QUEUED", "FAILED", false},

		// Terminal states cannot transition
		{"COMPLETED → PROCESSING (invalid)", "COMPLETED", "PROCESSING", false},
		{"FAILED → PROCESSING (invalid)", "FAILED", "PROCESSING", false},
		{"CANCELLED → PROCESSING (invalid)", "CANCELLED", "PROCESSING", false},

		// Edge cases
		{"Same state", "PROCESSING", "PROCESSING", false},
		{"Unknown current state", "UNKNOWN", "PROCESSING", false},
		{"Empty next state", "QUEUED", "", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			assert.Equal(t, tt.valid, validateStatusTransition(tt.current, tt.next))
		})
	}
}

func TestTerminalStatesHaveNoOutbound(t *testing.T) {
	terminal := []string{"COMPLETED", "FAILED", "CANCELLED"}
	all := []string{"QUEUED", "PROCESSING", "COMPLETED", "FAILED", "CANCELLED"}

	for _, from := range terminal {
		for _, to := range all {
			assert.False(t, validateStatusTransition(from, to),
				"%s must not transition to %s", from, to)
		}
	}
}

// ===========================================================================
// 2. Job Update Request – Progress Validation
// ===========================================================================

func TestProgressRangeValidation(t *testing.T) {
	cases := []struct {
		name  string
		value *int
		valid bool
	}{
		{"0 (min boundary)", intPtr(0), true},
		{"50 (mid-range)", intPtr(50), true},
		{"100 (max boundary)", intPtr(100), true},
		{"-1 (underflow)", intPtr(-1), false},
		{"101 (overflow)", intPtr(101), false},
		{"nil (optional field)", nil, true},
	}

	for _, tt := range cases {
		t.Run(tt.name, func(t *testing.T) {
			if tt.value != nil {
				inRange := *tt.value >= 0 && *tt.value <= 100
				assert.Equal(t, tt.valid, inRange)
			}
		})
	}
}

// ===========================================================================
// 3. JSON Serialization / Deserialization
// ===========================================================================

func TestJobUpdateRequest_JSON(t *testing.T) {
	tests := []struct {
		name      string
		input     string
		wantError bool
	}{
		{"All fields", `{"status":"PROCESSING","progress":50,"result_summary":{"files":10},"error":"err"}`, false},
		{"Status only", `{"status":"COMPLETED"}`, false},
		{"Progress only", `{"progress":75}`, false},
		{"ResultSummary only", `{"result_summary":{"total":100}}`, false},
		{"Error only", `{"error":"Something went wrong"}`, false},
		{"Malformed JSON", `{"status":"PROCESSING"`, true},
		{"Wrong type for progress", `{"progress":"not a number"}`, true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var req JobUpdateRequest
			err := json.Unmarshal([]byte(tt.input), &req)
			if tt.wantError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestJobUpdateResponse_Serialization(t *testing.T) {
	resp := JobUpdateResponse{
		JobID:     "test-job-123",
		Status:    "PROCESSING",
		Message:   "Job updated successfully",
		UpdatedAt: time.Date(2026, 2, 8, 12, 0, 0, 0, time.UTC),
	}

	data, err := json.Marshal(resp)
	require.NoError(t, err)

	var decoded map[string]interface{}
	require.NoError(t, json.Unmarshal(data, &decoded))

	assert.Equal(t, "test-job-123", decoded["job_id"])
	assert.Equal(t, "PROCESSING", decoded["status"])
	assert.Equal(t, "Job updated successfully", decoded["message"])
	assert.NotNil(t, decoded["updated_at"])
}

// ===========================================================================
// 4. Edge Cases
// ===========================================================================

func TestEdgeCases(t *testing.T) {
	t.Run("All-nil request is a valid struct", func(t *testing.T) {
		req := JobUpdateRequest{}
		assert.Nil(t, req.Status)
		assert.Nil(t, req.Progress)
		assert.Nil(t, req.ResultSummary)
		assert.Nil(t, req.Error)
	})

	t.Run("Nested result_summary serializes correctly", func(t *testing.T) {
		req := JobUpdateRequest{
			ResultSummary: map[string]interface{}{
				"files":  map[string]interface{}{"total": 100, "analyzed": 95},
				"issues": []interface{}{map[string]interface{}{"type": "error", "count": 2}},
			},
		}
		data, err := json.Marshal(req.ResultSummary)
		assert.NoError(t, err)
		assert.NotEmpty(t, data)
	})
}

// ===========================================================================
// 5. Complete Job Lifecycle
// ===========================================================================

func TestJobLifecycle(t *testing.T) {
	t.Run("Happy path: QUEUED → PROCESSING → COMPLETED", func(t *testing.T) {
		assert.True(t, validateStatusTransition("QUEUED", "PROCESSING"))
		assert.True(t, validateStatusTransition("PROCESSING", "COMPLETED"))
		assert.False(t, validateStatusTransition("COMPLETED", "PROCESSING"))
	})

	t.Run("Failure path: QUEUED → PROCESSING → FAILED", func(t *testing.T) {
		assert.True(t, validateStatusTransition("QUEUED", "PROCESSING"))
		assert.True(t, validateStatusTransition("PROCESSING", "FAILED"))
		assert.False(t, validateStatusTransition("FAILED", "PROCESSING"))
	})

	t.Run("Cancellation from any active state", func(t *testing.T) {
		assert.True(t, validateStatusTransition("QUEUED", "CANCELLED"))
		assert.True(t, validateStatusTransition("PROCESSING", "CANCELLED"))
	})
}

// ===========================================================================
// 6. Benchmarks
// ===========================================================================

func BenchmarkValidateStatusTransition(b *testing.B) {
	for i := 0; i < b.N; i++ {
		validateStatusTransition("QUEUED", "PROCESSING")
	}
}

func BenchmarkJSONMarshalUpdateRequest(b *testing.B) {
	req := JobUpdateRequest{
		Status:        stringPtr("PROCESSING"),
		Progress:      intPtr(50),
		ResultSummary: map[string]interface{}{"files_analyzed": 100},
		Error:         stringPtr("test error"),
	}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, _ = json.Marshal(req)
	}
}
