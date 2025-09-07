package shell_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/lib/shell"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestHandler_ToOperations(t *testing.T) {
	handler := shell.NewHandler()

	tests := []struct {
		name     string
		matches  []operations.FileInput
		wantOps  int
		checkOps func(*testing.T, []operations.Operation)
	}{
		{
			name: "single shell script creates one operation",
			matches: []operations.FileInput{
				{
					PackName:     "bash",
					RelativePath: "aliases.sh",
					SourcePath:   "/dotfiles/bash/aliases.sh",
				},
			},
			wantOps: 1,
			checkOps: func(t *testing.T, ops []operations.Operation) {
				assert.Equal(t, operations.CreateDataLink, ops[0].Type)
				assert.Equal(t, "bash", ops[0].Pack)
				assert.Equal(t, "shell", ops[0].Handler)
				assert.Equal(t, "/dotfiles/bash/aliases.sh", ops[0].Source)
			},
		},
		{
			name: "multiple shell scripts create multiple operations",
			matches: []operations.FileInput{
				{
					PackName:     "bash",
					RelativePath: "aliases.sh",
					SourcePath:   "/dotfiles/bash/aliases.sh",
				},
				{
					PackName:     "bash",
					RelativePath: "functions.sh",
					SourcePath:   "/dotfiles/bash/functions.sh",
				},
				{
					PackName:     "zsh",
					RelativePath: "config.zsh",
					SourcePath:   "/dotfiles/zsh/config.zsh",
				},
			},
			wantOps: 3,
			checkOps: func(t *testing.T, ops []operations.Operation) {
				// All should be CreateDataLink operations
				for _, op := range ops {
					assert.Equal(t, operations.CreateDataLink, op.Type)
					assert.Equal(t, "shell", op.Handler)
				}

				// Check specific operations
				assert.Equal(t, "bash", ops[0].Pack)
				assert.Equal(t, "/dotfiles/bash/aliases.sh", ops[0].Source)

				assert.Equal(t, "bash", ops[1].Pack)
				assert.Equal(t, "/dotfiles/bash/functions.sh", ops[1].Source)

				assert.Equal(t, "zsh", ops[2].Pack)
				assert.Equal(t, "/dotfiles/zsh/config.zsh", ops[2].Source)
			},
		},
		{
			name:    "empty matches returns empty operations",
			matches: []operations.FileInput{},
			wantOps: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ops, err := handler.ToOperations(tt.matches, nil)

			require.NoError(t, err)
			assert.Len(t, ops, tt.wantOps)

			if tt.checkOps != nil {
				tt.checkOps(t, ops)
			}
		})
	}
}

func TestHandler_GetMetadata(t *testing.T) {
	handler := shell.NewHandler()
	meta := handler.GetMetadata()

	assert.Equal(t, "Manages shell profile modifications (e.g., sourcing aliases)", meta.Description)
	assert.False(t, meta.RequiresConfirm)
	assert.True(t, meta.CanRunMultiple)
}

func TestHandler_GetTemplateContent(t *testing.T) {
	handler := shell.NewHandler()

	// Template should not be empty
	template := handler.GetTemplateContent()
	assert.NotEmpty(t, template)

	// Should contain shell script content
	assert.Contains(t, template, "#")
}
