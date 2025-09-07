package path_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/lib/path"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestHandler_ToOperations(t *testing.T) {
	tests := []struct {
		name        string
		files       []operations.FileInput
		expectedOps int
		checkFunc   func(*testing.T, []operations.Operation)
	}{
		{
			name: "single directory creates one operation",
			files: []operations.FileInput{
				{
					PackName:     "tools",
					RelativePath: "bin",
					SourcePath:   "/dotfiles/tools/bin",
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
			files: []operations.FileInput{
				{
					PackName:     "tools",
					RelativePath: "bin",
					SourcePath:   "/dotfiles/tools/bin",
				},
				{
					PackName:     "tools",
					RelativePath: "scripts",
					SourcePath:   "/dotfiles/tools/scripts",
				},
				{
					PackName:     "dev",
					RelativePath: "bin",
					SourcePath:   "/dotfiles/dev/bin",
				},
			},
			expectedOps: 3,
			checkFunc: func(t *testing.T, ops []operations.Operation) {
				// All should be CreateDataLink operations
				for _, op := range ops {
					assert.Equal(t, operations.CreateDataLink, op.Type)
					assert.Equal(t, "path", op.Handler)
				}

				// Check we have the right directories
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
			files: []operations.FileInput{
				{
					PackName:     "tools",
					RelativePath: "bin",
					SourcePath:   "/dotfiles/tools/bin",
				},
				{
					PackName:     "tools",
					RelativePath: "bin",
					SourcePath:   "/dotfiles/tools/bin",
				},
				{
					PackName:     "tools",
					RelativePath: "bin",
					SourcePath:   "/dotfiles/tools/bin",
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
			files: []operations.FileInput{
				{
					PackName:     "tools",
					RelativePath: "bin",
					SourcePath:   "/dotfiles/tools/bin",
				},
				{
					PackName:     "dev",
					RelativePath: "bin",
					SourcePath:   "/dotfiles/dev/bin",
				},
			},
			expectedOps: 2, // Same path but different packs
			checkFunc: func(t *testing.T, ops []operations.Operation) {
				packs := make(map[string]bool)
				for _, op := range ops {
					assert.Equal(t, "bin", op.Source)
					packs[op.Pack] = true
				}
				assert.Len(t, packs, 2, "Should have two different packs")
				assert.Contains(t, packs, "tools")
				assert.Contains(t, packs, "dev")
			},
		},
		{
			name:        "empty matches create no operations",
			files:       []operations.FileInput{},
			expectedOps: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			handler := path.NewHandler()

			// Execute
			ops, err := handler.ToOperations(tt.files, nil)

			// Verify
			require.NoError(t, err)
			assert.Len(t, ops, tt.expectedOps)

			if tt.checkFunc != nil {
				tt.checkFunc(t, ops)
			}
		})
	}
}

func TestHandler_Metadata(t *testing.T) {
	handler := path.NewHandler()

	// Test name
	assert.Equal(t, "path", handler.Name())

	// Test category
	assert.Equal(t, operations.CategoryConfiguration, handler.Category())

	// Test metadata
	meta := handler.GetMetadata()
	assert.Equal(t, "Adds directories to shell PATH", meta.Description)
	assert.False(t, meta.RequiresConfirm)
	assert.True(t, meta.CanRunMultiple)
}

func TestHandler_DefaultImplementations(t *testing.T) {
	handler := path.NewHandler()

	// These should use the base handler defaults
	assert.Nil(t, handler.GetClearConfirmation(operations.ClearContext{}))
	assert.Equal(t, "", handler.FormatClearedItem(operations.ClearedItem{}, false))
	assert.NoError(t, handler.ValidateOperations(nil))
	assert.Equal(t, "", handler.GetStateDirectoryName())
}
