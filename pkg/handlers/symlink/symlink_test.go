package symlink_test

import (
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestHandler_ToOperations(t *testing.T) {
	// Set HOME for consistent tests
	oldHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", "/home/testuser")
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	handler := symlink.NewHandler()

	tests := []struct {
		name        string
		matches     []types.RuleMatch
		wantOps     int
		checkOps    func(*testing.T, []operations.Operation)
		wantErr     bool
		errContains string
	}{
		{
			name: "single file creates two operations",
			matches: []types.RuleMatch{
				{
					Pack:         "vim",
					Path:         ".vimrc",
					AbsolutePath: "/dotfiles/vim/.vimrc",
					HandlerName:  "symlink",
				},
			},
			wantOps: 2,
			checkOps: func(t *testing.T, ops []operations.Operation) {
				// First operation: CreateDataLink
				assert.Equal(t, operations.CreateDataLink, ops[0].Type)
				assert.Equal(t, "vim", ops[0].Pack)
				assert.Equal(t, "symlink", ops[0].Handler)
				assert.Equal(t, "/dotfiles/vim/.vimrc", ops[0].Source)

				// Second operation: CreateUserLink
				assert.Equal(t, operations.CreateUserLink, ops[1].Type)
				assert.Equal(t, "vim", ops[1].Pack)
				assert.Equal(t, "/home/testuser/.vimrc", ops[1].Target)
			},
		},
		{
			name: "multiple files create paired operations",
			matches: []types.RuleMatch{
				{
					Pack:         "vim",
					Path:         ".vimrc",
					AbsolutePath: "/dotfiles/vim/.vimrc",
					HandlerName:  "symlink",
				},
				{
					Pack:         "vim",
					Path:         ".vim/colors/theme.vim",
					AbsolutePath: "/dotfiles/vim/.vim/colors/theme.vim",
					HandlerName:  "symlink",
				},
			},
			wantOps: 4, // 2 operations per file
			checkOps: func(t *testing.T, ops []operations.Operation) {
				// Check pairs
				assert.Equal(t, operations.CreateDataLink, ops[0].Type)
				assert.Equal(t, operations.CreateUserLink, ops[1].Type)
				assert.Equal(t, operations.CreateDataLink, ops[2].Type)
				assert.Equal(t, operations.CreateUserLink, ops[3].Type)

				// Check targets
				assert.Equal(t, "/home/testuser/.vimrc", ops[1].Target)
				assert.Equal(t, "/home/testuser/.vim/colors/theme.vim", ops[3].Target)
			},
		},
		{
			name: "custom target directory",
			matches: []types.RuleMatch{
				{
					Pack:         "configs",
					Path:         "app.conf",
					AbsolutePath: "/dotfiles/configs/app.conf",
					HandlerName:  "symlink",
					HandlerOptions: map[string]interface{}{
						"target": "/etc/myapp",
					},
				},
			},
			wantOps: 2,
			checkOps: func(t *testing.T, ops []operations.Operation) {
				assert.Equal(t, "/etc/myapp/app.conf", ops[1].Target)
			},
		},
		{
			name: "environment variable expansion in target",
			matches: []types.RuleMatch{
				{
					Pack:         "configs",
					Path:         "config.yml",
					AbsolutePath: "/dotfiles/configs/config.yml",
					HandlerName:  "symlink",
					HandlerOptions: map[string]interface{}{
						"target": "$HOME/.config",
					},
				},
			},
			wantOps: 2,
			checkOps: func(t *testing.T, ops []operations.Operation) {
				assert.Equal(t, "/home/testuser/.config/config.yml", ops[1].Target)
			},
		},
		{
			name: "conflict detection",
			matches: []types.RuleMatch{
				{
					Pack:         "vim",
					Path:         ".vimrc",
					AbsolutePath: "/dotfiles/vim/.vimrc",
					HandlerName:  "symlink",
				},
				{
					Pack:         "neovim",
					Path:         ".vimrc",
					AbsolutePath: "/dotfiles/neovim/.vimrc",
					HandlerName:  "symlink",
				},
			},
			wantErr:     true,
			errContains: "symlink conflict",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ops, err := handler.ToOperations(tt.matches)

			if tt.wantErr {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.errContains)
				return
			}

			require.NoError(t, err)
			assert.Len(t, ops, tt.wantOps)

			if tt.checkOps != nil {
				tt.checkOps(t, ops)
			}
		})
	}
}

func TestHandler_GetMetadata(t *testing.T) {
	handler := symlink.NewHandler()
	meta := handler.GetMetadata()

	assert.Equal(t, "Creates symbolic links from dotfiles to target locations", meta.Description)
	assert.False(t, meta.RequiresConfirm)
	assert.True(t, meta.CanRunMultiple)
}

func TestHandler_FormatClearedItem(t *testing.T) {
	handler := symlink.NewHandler()

	item := types.ClearedItem{
		Type: "symlink",
		Path: "/home/user/.vimrc",
	}

	// Dry run
	formatted := handler.FormatClearedItem(item, true)
	assert.Equal(t, "Would remove symlink .vimrc", formatted)

	// Actual run
	formatted = handler.FormatClearedItem(item, false)
	assert.Equal(t, "Removed symlink .vimrc", formatted)
}
