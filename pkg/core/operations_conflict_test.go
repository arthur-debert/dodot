package core

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDetectOperationConflicts(t *testing.T) {
	tests := []struct {
		name      string
		ops       []types.Operation
		wantError bool
		errorMsg  string
	}{
		{
			name: "no conflicts - different targets",
			ops: []types.Operation{
				{
					Type:        types.OperationCreateSymlink,
					Target:      "/home/user/.vimrc",
					Description: "Symlink .vimrc",
				},
				{
					Type:        types.OperationCreateSymlink,
					Target:      "/home/user/.bashrc",
					Description: "Symlink .bashrc",
				},
			},
			wantError: false,
		},
		{
			name: "no conflicts - multiple dir creates",
			ops: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      "/home/user/.config",
					Description: "Create config dir for pack1",
				},
				{
					Type:        types.OperationCreateDir,
					Target:      "/home/user/.config",
					Description: "Create config dir for pack2",
				},
			},
			wantError: false,
		},
		{
			name: "conflict - multiple symlinks to same target",
			ops: []types.Operation{
				{
					Type:        types.OperationCreateSymlink,
					Target:      "/home/user/.gitconfig",
					Description: "Symlink from pack1",
				},
				{
					Type:        types.OperationCreateSymlink,
					Target:      "/home/user/.gitconfig",
					Description: "Symlink from pack2",
				},
			},
			wantError: true,
			errorMsg:  "Multiple operations target /home/user/.gitconfig",
		},
		{
			name: "conflict - symlink and write to same target",
			ops: []types.Operation{
				{
					Type:        types.OperationCreateSymlink,
					Target:      "/home/user/.zshrc",
					Description: "Symlink .zshrc",
				},
				{
					Type:        types.OperationWriteFile,
					Target:      "/home/user/.zshrc",
					Description: "Write .zshrc from template",
				},
			},
			wantError: true,
			errorMsg:  "Multiple operations target /home/user/.zshrc",
		},
		{
			name: "conflict - multiple writes to same target",
			ops: []types.Operation{
				{
					Type:        types.OperationWriteFile,
					Target:      "/home/user/.config/app.conf",
					Description: "Write config from template1",
				},
				{
					Type:        types.OperationWriteFile,
					Target:      "/home/user/.config/app.conf",
					Description: "Write config from template2",
				},
			},
			wantError: true,
			errorMsg:  "Multiple operations target /home/user/.config/app.conf",
		},
		{
			name: "conflict - copy and write to same target",
			ops: []types.Operation{
				{
					Type:        types.OperationCopyFile,
					Target:      "/home/user/scripts/backup.sh",
					Description: "Copy backup script",
				},
				{
					Type:        types.OperationWriteFile,
					Target:      "/home/user/scripts/backup.sh",
					Description: "Generate backup script",
				},
			},
			wantError: true,
			errorMsg:  "Multiple operations target /home/user/scripts/backup.sh",
		},
		{
			name: "no conflict - operations without targets",
			ops: []types.Operation{
				{
					Type:        types.OperationCreateSymlink,
					Target:      "/home/user/.vimrc",
					Description: "Symlink .vimrc",
				},
				{
					Type:        "some_operation",
					Target:      "", // Empty target should be skipped
					Description: "Operation without target",
				},
			},
			wantError: false,
		},
		{
			name: "conflict - normalized paths",
			ops: []types.Operation{
				{
					Type:        types.OperationCreateSymlink,
					Target:      "/home/user/.config/../.bashrc",
					Description: "Symlink with relative path",
				},
				{
					Type:        types.OperationWriteFile,
					Target:      "/home/user/.bashrc",
					Description: "Write to normalized path",
				},
			},
			wantError: true,
			errorMsg:  "Multiple operations target /home/user/.bashrc",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := detectOperationConflicts(tt.ops)

			if tt.wantError {
				assert.Error(t, err)
				if tt.errorMsg != "" {
					assert.Contains(t, err.Error(), tt.errorMsg)
				}
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestGetFsOps_WithConflictDetection(t *testing.T) {
	// Test that GetFsOps properly detects conflicts
	actions := []types.Action{
		{
			Type:        types.ActionTypeLink,
			Source:      "/dotfiles/vim/.vimrc",
			Target:      "~/.vimrc",
			Description: "Symlink .vimrc from vim pack",
			Pack:        "vim",
			Priority:    100,
		},
		{
			Type:        types.ActionTypeWrite,
			Target:      "~/.vimrc",
			Content:     "\" Generated vimrc",
			Description: "Write .vimrc from template",
			Pack:        "vim-template",
			Priority:    90,
		},
	}

	ops, err := GetFsOps(actions)
	require.Error(t, err)
	assert.Nil(t, ops)
	assert.Contains(t, err.Error(), "conflicts")
	assert.Contains(t, err.Error(), ".vimrc")
}

func TestAreOperationsCompatible(t *testing.T) {
	tests := []struct {
		name       string
		ops        []types.Operation
		compatible bool
	}{
		{
			name: "multiple dir creates are compatible",
			ops: []types.Operation{
				{Type: types.OperationCreateDir},
				{Type: types.OperationCreateDir},
				{Type: types.OperationCreateDir},
			},
			compatible: true,
		},
		{
			name: "dir create with other operation is incompatible",
			ops: []types.Operation{
				{Type: types.OperationCreateDir},
				{Type: types.OperationCreateSymlink},
			},
			compatible: false,
		},
		{
			name: "multiple symlinks are incompatible",
			ops: []types.Operation{
				{Type: types.OperationCreateSymlink},
				{Type: types.OperationCreateSymlink},
			},
			compatible: false,
		},
		{
			name: "symlink and write are incompatible",
			ops: []types.Operation{
				{Type: types.OperationCreateSymlink},
				{Type: types.OperationWriteFile},
			},
			compatible: false,
		},
		{
			name: "single operation is compatible with itself",
			ops: []types.Operation{
				{Type: types.OperationWriteFile},
			},
			compatible: true, // This won't be called with single op, but should handle gracefully
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := areOperationsCompatible(tt.ops)
			assert.Equal(t, tt.compatible, result)
		})
	}
}
