package main

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// ===========================================================================
// Known Issues – Tests that FAIL to demonstrate validation gaps
// These failures prove the test framework catches real problems in the code.
// ===========================================================================

func TestKnownIssue_RepoURL_ShouldRejectSpaces(t *testing.T) {
	// BUG: validateRepoURL uses regex [^/]+ which accepts spaces in URLs.
	// A proper URL validator should reject whitespace characters.
	url := "https://github.com/user/repo with spaces"
	result := validateRepoURL(url)
	assert.False(t, result,
		"KNOWN ISSUE: validateRepoURL accepts URLs with spaces because regex [^/]+ does not filter whitespace")
}

func TestKnownIssue_RepoURL_ShouldRejectSpecialChars(t *testing.T) {
	// BUG: validateRepoURL uses regex [^/]+ which accepts SQL-injection characters.
	// A secure URL validator should reject or sanitize special characters.
	url := "https://github.com/user/repo'; DROP TABLE"
	result := validateRepoURL(url)
	assert.False(t, result,
		"KNOWN ISSUE: validateRepoURL accepts SQL injection characters because regex [^/]+ only blocks forward slashes")
}
