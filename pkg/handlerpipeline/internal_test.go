package handlerpipeline

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestGroupMatchesByHandler(t *testing.T) {
	matches := []types.RuleMatch{
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
