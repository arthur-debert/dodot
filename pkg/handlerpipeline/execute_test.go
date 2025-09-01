package handlerpipeline

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

// For now, we'll just test the basic structure and filter logic
// Full integration tests will come when we integrate with the test environment

func TestFilterType_Constants(t *testing.T) {
	// Verify filter types are distinct
	assert.NotEqual(t, ConfigOnly, ProvisionOnly)
	assert.NotEqual(t, ConfigOnly, All)
	assert.NotEqual(t, ProvisionOnly, All)
}

func TestFilterTypeString(t *testing.T) {
	tests := []struct {
		filter   FilterType
		expected string
	}{
		{ConfigOnly, "ConfigOnly"},
		{ProvisionOnly, "ProvisionOnly"},
		{All, "All"},
		{FilterType(99), "Unknown"},
	}

	for _, tt := range tests {
		t.Run(tt.expected, func(t *testing.T) {
			assert.Equal(t, tt.expected, filterTypeString(tt.filter))
		})
	}
}

func TestBuildResultFromContext_Empty(t *testing.T) {
	pack := types.Pack{Name: "test"}

	// Test with nil context
	result := buildResultFromContext(pack, nil)
	assert.NotNil(t, result)
	assert.Equal(t, "test", result.Pack.Name)
	assert.Equal(t, 0, result.TotalHandlers)
	assert.Empty(t, result.ExecutedHandlers)
}

func TestBuildResultFromContext_WithHandlers(t *testing.T) {
	pack := types.Pack{Name: "test"}
	ctx := types.NewExecutionContext("test", false)

	// Create pack result
	packResult := types.NewPackExecutionResult(&pack)

	// Add handler results
	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "symlink",
		Status:      types.StatusReady,
		Files:       []string{"file1", "file2"},
	})

	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "shell",
		Status:      types.StatusSkipped,
		Files:       []string{"file3"},
	})

	packResult.AddHandlerResult(&types.HandlerResult{
		HandlerName: "homebrew",
		Status:      types.StatusError,
		Files:       []string{"Brewfile"},
	})

	ctx.AddPackResult("test", packResult)

	// Build result
	result := buildResultFromContext(pack, ctx)

	// Verify counts
	assert.Equal(t, 3, result.TotalHandlers)
	assert.Equal(t, 1, result.SuccessCount)
	assert.Equal(t, 1, result.SkippedCount)
	assert.Equal(t, 1, result.FailureCount)

	// Verify handler executions
	assert.Len(t, result.ExecutedHandlers, 3)

	// Check first handler
	assert.Equal(t, "symlink", result.ExecutedHandlers[0].HandlerName)
	assert.Equal(t, 2, result.ExecutedHandlers[0].OperationCount)
	assert.True(t, result.ExecutedHandlers[0].Success)

	// Check skipped handler
	assert.Equal(t, "shell", result.ExecutedHandlers[1].HandlerName)
	assert.False(t, result.ExecutedHandlers[1].Success)

	// Check failed handler
	assert.Equal(t, "homebrew", result.ExecutedHandlers[2].HandlerName)
	assert.False(t, result.ExecutedHandlers[2].Success)
}

// TODO: Add integration tests once we have proper test environment support
// These will test the full ExecuteHandlersForPack function with:
// - Mock filesystem
// - Mock datastore
// - Proper rule matching
