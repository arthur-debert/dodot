package path_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSimplifiedHandler_ToOperations(t *testing.T) {
	tests := []struct {
		name        string
		matches     []types.RuleMatch
		expectedOps int
		checkFunc   func(*testing.T, []operations.Operation)
	}{
		{
			name: "single directory creates one operation",
			matches: []types.RuleMatch{
				{
					Pack:        "tools",
					Path:        "bin",
					HandlerName: "path",
				},
			},
			expectedOps: 1,
			checkFunc: func(t *testing.T, ops []operations.Operation) {
				assert.Equal(t, operations.CreateDataLink, ops[0].Type)
				assert.Equal(t, "tools", ops[0].Pack)
				assert.Equal(t, "path", ops[0].Handler)
				assert.Equal(t, "bin", ops[0].Source)
			},
		},
		{
			name: "multiple directories create multiple operations",
			matches: []types.RuleMatch{
				{
					Pack:        "tools",
					Path:        "bin",
					HandlerName: "path",
				},
				{
					Pack:        "tools",
					Path:        "scripts",
					HandlerName: "path",
				},
				{
					Pack:        "dev",
					Path:        "bin",
					HandlerName: "path",
				},
			},
			expectedOps: 3,
			checkFunc: func(t *testing.T, ops []operations.Operation) {
				// All should be CreateDataLink operations
				for _, op := range ops {
					assert.Equal(t, operations.CreateDataLink, op.Type)
					assert.Equal(t, "path", op.Handler)
				}

				// Check specific operations exist
				foundToolsBin := false
				foundToolsScripts := false
				foundDevBin := false

				for _, op := range ops {
					if op.Pack == "tools" && op.Source == "bin" {
						foundToolsBin = true
					}
					if op.Pack == "tools" && op.Source == "scripts" {
						foundToolsScripts = true
					}
					if op.Pack == "dev" && op.Source == "bin" {
						foundDevBin = true
					}
				}

				assert.True(t, foundToolsBin, "Should have tools/bin")
				assert.True(t, foundToolsScripts, "Should have tools/scripts")
				assert.True(t, foundDevBin, "Should have dev/bin")
			},
		},
		{
			name: "duplicate paths are deduplicated",
			matches: []types.RuleMatch{
				{
					Pack:        "tools",
					Path:        "bin",
					HandlerName: "path",
				},
				{
					Pack:        "tools",
					Path:        "bin",
					HandlerName: "path",
				},
				{
					Pack:        "tools",
					Path:        "bin",
					HandlerName: "path",
				},
			},
			expectedOps: 1, // Only one operation despite 3 matches
			checkFunc: func(t *testing.T, ops []operations.Operation) {
				assert.Equal(t, "tools", ops[0].Pack)
				assert.Equal(t, "bin", ops[0].Source)
			},
		},
		{
			name: "same path in different packs creates separate operations",
			matches: []types.RuleMatch{
				{
					Pack:        "tools",
					Path:        "bin",
					HandlerName: "path",
				},
				{
					Pack:        "dev",
					Path:        "bin",
					HandlerName: "path",
				},
			},
			expectedOps: 2, // Same path but different packs
			checkFunc: func(t *testing.T, ops []operations.Operation) {
				packs := make(map[string]bool)
				for _, op := range ops {
					assert.Equal(t, "bin", op.Source)
					packs[op.Pack] = true
				}
				assert.True(t, packs["tools"])
				assert.True(t, packs["dev"])
			},
		},
		{
			name:        "empty matches creates empty operations",
			matches:     []types.RuleMatch{},
			expectedOps: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			handler := path.NewSimplifiedHandler()

			ops, err := handler.ToOperations(tt.matches)
			require.NoError(t, err)
			assert.Len(t, ops, tt.expectedOps)

			if tt.checkFunc != nil {
				tt.checkFunc(t, ops)
			}
		})
	}
}

func TestSimplifiedHandler_Metadata(t *testing.T) {
	handler := path.NewSimplifiedHandler()

	// Test basic properties
	assert.Equal(t, "path", handler.Name())
	assert.Equal(t, handlers.CategoryConfiguration, handler.Category())

	// Test metadata
	metadata := handler.GetMetadata()
	assert.Equal(t, "Adds directories to shell PATH", metadata.Description)
	assert.False(t, metadata.RequiresConfirm) // PATH additions are safe
	assert.True(t, metadata.CanRunMultiple)   // Can add multiple directories

	// Test that optional methods use defaults
	assert.Nil(t, handler.GetClearConfirmation(types.ClearContext{}))
	assert.Empty(t, handler.FormatClearedItem(types.ClearedItem{}, false))
	assert.NoError(t, handler.ValidateOperations(nil))
	assert.Empty(t, handler.GetStateDirectoryName())
}

func TestSimplifiedHandler_ComparisonWithCurrent(t *testing.T) {
	// This test documents the simplification achieved

	// Current handler has these responsibilities:
	// 1. ProcessLinking with backward compatibility     - REMOVED (operation executor handles)
	// 2. ProcessLinkingWithConfirmations              - REMOVED (operation executor handles)
	// 3. Complex options parsing                      - REMOVED (not needed)
	// 4. Path resolution and validation               - REMOVED (datastore handles)
	// 5. Action creation with all fields              - SIMPLIFIED (just operations)
	// 6. Logging throughout                           - REMOVED (executor handles)
	// 7. State management                             - REMOVED (datastore handles)
	// 8. Clear method implementation                  - REMOVED (generic reversal)

	// New handler only has:
	// 1. Name and category
	// 2. Metadata for UI
	// 3. Match to operation transformation
	// 4. Optional customization points

	// Result: ~185 lines → ~40 lines (78% reduction)
}
