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

func TestGetHandlerExecutionOrder(t *testing.T) {
	tests := []struct {
		name     string
		handlers []string
		expected []string
	}{
		{
			name:     "code execution before configuration",
			handlers: []string{"symlink", "homebrew", "shell", "install"},
			expected: []string{"homebrew", "install", "symlink", "shell"},
		},
		{
			name:     "only configuration handlers",
			handlers: []string{"symlink", "shell", "path"},
			expected: []string{"symlink", "shell", "path"},
		},
		{
			name:     "only code execution handlers",
			handlers: []string{"homebrew", "install"},
			expected: []string{"homebrew", "install"},
		},
		{
			name:     "empty list",
			handlers: []string{},
			expected: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := getHandlerExecutionOrder(tt.handlers)
			assert.Equal(t, tt.expected, result)
		})
	}
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

func TestCreateOperationsHandler(t *testing.T) {
	tests := []struct {
		name        string
		handlerName string
		shouldError bool
	}{
		{"symlink handler", "symlink", false},
		{"shell handler", "shell", false},
		{"homebrew handler", "homebrew", false},
		{"install handler", "install", false},
		{"path handler", "path", false},
		{"unknown handler", "unknown", true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			handler, err := createOperationsHandler(tt.handlerName)
			if tt.shouldError {
				assert.Error(t, err)
				assert.Nil(t, handler)
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, handler)
			}
		})
	}
}
