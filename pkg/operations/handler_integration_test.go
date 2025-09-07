package operations_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/lib/path"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestPathHandler_OperationIntegration(t *testing.T) {
	// This test verifies the path handler works with the operation system

	// Create simplified handler
	handler := path.NewHandler()

	// Create test file inputs
	files := []operations.FileInput{
		{
			PackName:     "tools",
			SourcePath:   "/test/tools/bin",
			RelativePath: "bin",
		},
		{
			PackName:     "tools",
			SourcePath:   "/test/tools/scripts",
			RelativePath: "scripts",
		},
		{
			PackName:     "dev",
			SourcePath:   "/test/dev/bin",
			RelativePath: "bin",
		},
	}

	// Convert to operations
	ops, err := handler.ToOperations(files, nil)
	require.NoError(t, err)
	assert.Len(t, ops, 3)

	// Verify operations
	for _, op := range ops {
		assert.Equal(t, operations.CreateDataLink, op.Type)
		assert.Equal(t, "path", op.Handler)
		assert.NotEmpty(t, op.Pack)
		assert.NotEmpty(t, op.Source)
	}

	// Test with executor in dry-run mode
	store := new(MockSimpleDataStore)
	executor := operations.NewExecutor(store, nil, true)

	// Execute operations
	results, err := executor.Execute(ops, handler)
	require.NoError(t, err)
	assert.Len(t, results, 3)

	// All should be successful in dry run
	for _, result := range results {
		assert.True(t, result.Success)
		assert.Contains(t, result.Message, "Would create")
	}
}

func TestPathHandler_Clear(t *testing.T) {
	// Test clear functionality
	handler := path.NewHandler()

	// Create mock store and executor
	store := new(MockSimpleDataStore)
	executor := operations.NewExecutor(store, nil, false)

	// Clear context
	ctx := operations.ClearContext{
		Pack: types.Pack{
			Name: "tools",
		},
		DryRun: true,
	}

	// Execute clear
	clearedItems, err := executor.ExecuteClear(handler, ctx)
	require.NoError(t, err)
	assert.Len(t, clearedItems, 1)

	// Check cleared item
	item := clearedItems[0]
	assert.Equal(t, "path_state", item.Type)
	assert.Contains(t, item.Path, "tools/path")
	assert.Contains(t, item.Description, "Would remove path state")
}
