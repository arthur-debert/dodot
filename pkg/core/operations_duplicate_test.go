//go:build unit
// +build unit

package core_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDuplicateParentDirectoryOperations(t *testing.T) {
	t.Run("multiple symlinks to same directory should not generate duplicate parent dir operations", func(t *testing.T) {
		// Create multiple symlink actions targeting the same parent directory
		actions := []types.Action{
			{
				Type:        types.ActionTypeLink,
				Source:      "/dotfiles/bashrc",
				Target:      "~/.bashrc",
				Description: "Symlink .bashrc",
			},
			{
				Type:        types.ActionTypeLink,
				Source:      "/dotfiles/zshrc",
				Target:      "~/.zshrc",
				Description: "Symlink .zshrc",
			},
			{
				Type:        types.ActionTypeLink,
				Source:      "/dotfiles/vimrc",
				Target:      "~/.vimrc",
				Description: "Symlink .vimrc",
			},
		}

		// Convert actions to operations
		operations, err := core.ConvertActionsToOperations(actions)
		require.NoError(t, err)

		// Count parent directory creation operations
		parentDirOps := 0
		var parentDirTargets []string
		for _, op := range operations {
			if op.Type == types.OperationCreateDir && op.Description == "Create parent directory for .bashrc" ||
				op.Description == "Create parent directory for .zshrc" ||
				op.Description == "Create parent directory for .vimrc" {
				parentDirOps++
				parentDirTargets = append(parentDirTargets, op.Target)
			}
		}

		// We should only have one parent directory creation operation
		assert.Equal(t, 1, parentDirOps, "Should only create parent directory once")

		// Log all operations for debugging
		t.Logf("Total operations: %d", len(operations))
		for i, op := range operations {
			t.Logf("Operation %d: Type=%s, Target=%s, Description=%s", i, op.Type, op.Target, op.Description)
		}
	})

	t.Run("symlinks to different directories should create separate parent dir operations", func(t *testing.T) {
		// Create symlink actions targeting different parent directories
		actions := []types.Action{
			{
				Type:        types.ActionTypeLink,
				Source:      "/dotfiles/bashrc",
				Target:      "~/.bashrc",
				Description: "Symlink .bashrc",
			},
			{
				Type:        types.ActionTypeLink,
				Source:      "/dotfiles/config/nvim/init.vim",
				Target:      "~/.config/nvim/init.vim",
				Description: "Symlink nvim config",
			},
		}

		// Convert actions to operations
		operations, err := core.ConvertActionsToOperations(actions)
		require.NoError(t, err)

		// Count unique parent directory targets
		parentDirTargets := make(map[string]bool)
		for _, op := range operations {
			if op.Type == types.OperationCreateDir &&
				(op.Description == "Create parent directory for .bashrc" ||
					op.Description == "Create parent directory for init.vim") {
				parentDirTargets[op.Target] = true
			}
		}

		// We should have 2 different parent directory creation operations
		assert.Equal(t, 2, len(parentDirTargets), "Should create two different parent directories")
	})
}
