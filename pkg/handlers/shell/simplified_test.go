package shell_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/shell"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSimplifiedHandler_ToOperations(t *testing.T) {
	handler := shell.NewSimplifiedHandler()

	tests := []struct {
		name     string
		matches  []types.RuleMatch
		wantOps  int
		checkOps func(*testing.T, []operations.Operation)
	}{
		{
			name: "single shell script creates one operation",
			matches: []types.RuleMatch{
				{
					Pack:         "bash",
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/bash/aliases.sh",
					HandlerName:  "shell",
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
			matches: []types.RuleMatch{
				{
					Pack:         "bash",
					Path:         "aliases.sh",
					AbsolutePath: "/dotfiles/bash/aliases.sh",
					HandlerName:  "shell",
				},
				{
					Pack:         "bash",
					Path:         "functions.sh",
					AbsolutePath: "/dotfiles/bash/functions.sh",
					HandlerName:  "shell",
				},
				{
					Pack:         "zsh",
					Path:         "config.zsh",
					AbsolutePath: "/dotfiles/zsh/config.zsh",
					HandlerName:  "shell",
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
			matches: []types.RuleMatch{},
			wantOps: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ops, err := handler.ToOperations(tt.matches)

			require.NoError(t, err)
			assert.Len(t, ops, tt.wantOps)

			if tt.checkOps != nil {
				tt.checkOps(t, ops)
			}
		})
	}
}

func TestSimplifiedHandler_GetMetadata(t *testing.T) {
	handler := shell.NewSimplifiedHandler()
	meta := handler.GetMetadata()

	assert.Equal(t, "Manages shell profile modifications (e.g., sourcing aliases)", meta.Description)
	assert.False(t, meta.RequiresConfirm)
	assert.True(t, meta.CanRunMultiple)
}

func TestSimplifiedHandler_GetTemplateContent(t *testing.T) {
	handler := shell.NewSimplifiedHandler()

	// Template should not be empty
	template := handler.GetTemplateContent()
	assert.NotEmpty(t, template)

	// Should contain shell script content
	assert.Contains(t, template, "#")
}
