package main

import "testing"

func TestExtractGitHubJSONPayload(t *testing.T) {
	t.Run("application_json_body", func(t *testing.T) {
		body := []byte(`{"ref":"refs/heads/main"}`)
		payload, err := extractGitHubJSONPayload(body, "application/json")
		if err != nil {
			t.Fatalf("expected no error, got %v", err)
		}
		if string(payload) != string(body) {
			t.Fatalf("expected payload %s, got %s", body, payload)
		}
	})

	t.Run("form_encoded_payload", func(t *testing.T) {
		body := []byte("payload=%7B%22ref%22%3A%22refs%2Fheads%2Fmain%22%7D")
		payload, err := extractGitHubJSONPayload(body, "application/x-www-form-urlencoded")
		if err != nil {
			t.Fatalf("expected no error, got %v", err)
		}
		expected := `{"ref":"refs/heads/main"}`
		if string(payload) != expected {
			t.Fatalf("expected payload %s, got %s", expected, payload)
		}
	})

	t.Run("form_payload_missing", func(t *testing.T) {
		_, err := extractGitHubJSONPayload([]byte("foo=bar"), "application/x-www-form-urlencoded")
		if err == nil {
			t.Fatal("expected error for missing payload field")
		}
	})
}
