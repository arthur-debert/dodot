package pipeline

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/stretchr/testify/assert"
)

func TestGroupMatchesByHandler(t *testing.T) {
	matches := []RuleMatch{
		{HandlerName: "symlink", Pack: "vim", Path: "vimrc"},
		{HandlerName: "symlink", Pack: "vim", Path: "gvimrc"},
		{HandlerName: "shell", Pack: "bash", Path: "profile.sh"},
		{HandlerName: "homebrew", Pack: "tools", Path: "Brewfile"},
	}

	grouped := groupMatchesByHandler(matches)

	assert.Len(t, grouped, 3, "should have 3 handler groups")
	assert.Len(t, grouped["symlink"], 2, "symlink should have 2 matches")
	assert.Len(t, grouped["shell"], 1, "shell should have 1 match")
	assert.Len(t, grouped["homebrew"], 1, "homebrew should have 1 match")
}

func TestCountSuccessfulResults(t *testing.T) {
	results := []operations.OperationResult{
		{Success: true},
		{Success: false, Error: nil},
		{Success: false, Error: assert.AnError},
		{Success: true},
	}

	count := countSuccessfulResults(results)
	assert.Equal(t, 2, count, "should count only successful results")

	// Test empty results
	assert.Equal(t, 0, countSuccessfulResults(nil))
	assert.Equal(t, 0, countSuccessfulResults([]operations.OperationResult{}))
}

func TestRuleMatch_Structure(t *testing.T) {
	match := RuleMatch{
		RuleName:     "filename",
		Pack:         "test-pack",
		Path:         "file.txt",
		AbsolutePath: "/test/file.txt",
		Priority:     10,
		Metadata: map[string]interface{}{
			"pattern": "*.txt",
		},
		HandlerName:    "symlink",
		HandlerOptions: map[string]interface{}{},
	}

	assert.Equal(t, "test-pack", match.Pack)
	assert.Equal(t, "file.txt", match.Path)
	assert.Equal(t, "/test/file.txt", match.AbsolutePath)
	assert.Equal(t, 10, match.Priority)
	assert.Equal(t, "filename", match.RuleName)
	assert.Equal(t, "symlink", match.HandlerName)

	// Check metadata
	assert.Contains(t, match.Metadata, "pattern")
	assert.Equal(t, "*.txt", match.Metadata["pattern"])
}
