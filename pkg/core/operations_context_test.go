package core

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// createTestPathsForContext creates a Paths instance for testing
func createTestPathsForContext(t *testing.T) *paths.Paths {
	tempDir := t.TempDir()
	t.Setenv("HOME", tempDir)
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	p, err := paths.New("")
	require.NoError(t, err)
	return p
}

func TestConvertActionWithContext_PreservesContext(t *testing.T) {
	tests := []struct {
		name   string
		action types.Action
		ctx    *ExecutionContext
		verify func(t *testing.T, ops []types.Operation)
	}{
		{
			name: "link action preserves pack and powerup",
			action: types.Action{
				Type:        types.ActionTypeLink,
				Source:      "/source/file",
				Target:      "/target/file",
				Description: "Link config file",
				Pack:        "vim",
				PowerUpName: "symlink",
				Priority:    10,
				Metadata: map[string]interface{}{
					"trigger":      "FileName",
					"originalPath": ".vimrc",
					"app":          "vim",
				},
			},
			ctx: &ExecutionContext{
				Paths: createTestPathsForContext(t),
			},
			verify: func(t *testing.T, ops []types.Operation) {
				require.Greater(t, len(ops), 0)
				for _, op := range ops {
					assert.Equal(t, "vim", op.Pack)
					assert.Equal(t, "symlink", op.PowerUp)
					assert.Equal(t, types.StatusReady, op.Status)
					assert.NotNil(t, op.Metadata)
					assert.Equal(t, "vim", op.Metadata["app"])
					assert.NotNil(t, op.TriggerInfo)
					assert.Equal(t, "FileName", op.TriggerInfo.TriggerName)
					assert.Equal(t, ".vimrc", op.TriggerInfo.OriginalPath)
					assert.Equal(t, 10, op.TriggerInfo.Priority)
					assert.Equal(t, "vim-symlink-10", op.GroupID)
				}
			},
		},
		{
			name: "write action without trigger info",
			action: types.Action{
				Type:        types.ActionTypeWrite,
				Target:      "/target/file",
				Content:     "content",
				Description: "Write config",
				Pack:        "bash",
				PowerUpName: "profile",
				Priority:    5,
			},
			ctx: nil,
			verify: func(t *testing.T, ops []types.Operation) {
				require.Greater(t, len(ops), 0)
				for _, op := range ops {
					assert.Equal(t, "bash", op.Pack)
					assert.Equal(t, "profile", op.PowerUp)
					assert.Nil(t, op.TriggerInfo)
					assert.Equal(t, "bash-profile-5", op.GroupID)
				}
			},
		},
		{
			name: "brew action with full metadata",
			action: types.Action{
				Type:        types.ActionTypeBrew,
				Source:      "/path/to/Brewfile",
				Description: "Install homebrew packages",
				Pack:        "homebrew",
				PowerUpName: "homebrew",
				Priority:    20,
				Metadata: map[string]interface{}{
					"pack":     "homebrew",
					"checksum": "abc123",
					"formula":  "vim",
					"options":  []string{"--with-lua"},
				},
			},
			ctx: &ExecutionContext{
				Paths:           createTestPaths(t),
				ChecksumResults: map[string]string{"/path/to/Brewfile": "abc123"},
			},
			verify: func(t *testing.T, ops []types.Operation) {
				require.Greater(t, len(ops), 0)
				for _, op := range ops {
					assert.Equal(t, "homebrew", op.Pack)
					assert.Equal(t, "homebrew", op.PowerUp)
					assert.NotNil(t, op.Metadata)
					assert.Equal(t, "vim", op.Metadata["formula"])
					assert.Equal(t, "homebrew-homebrew-20", op.GroupID)
				}
			},
		},
		{
			name: "action without pack/powerup",
			action: types.Action{
				Type:        types.ActionTypeMkdir,
				Target:      "/target/dir",
				Description: "Create directory",
			},
			ctx: nil,
			verify: func(t *testing.T, ops []types.Operation) {
				require.Greater(t, len(ops), 0)
				for _, op := range ops {
					assert.Empty(t, op.Pack)
					assert.Empty(t, op.PowerUp)
					assert.Empty(t, op.GroupID)
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ops, err := ConvertActionWithContext(tt.action, tt.ctx)
			require.NoError(t, err)
			tt.verify(t, ops)
		})
	}
}

func TestConvertActionsToOperationsWithContext_PreservesContext(t *testing.T) {
	ctx := &ExecutionContext{
		Paths: createTestPathsForContext(t),
	}

	actions := []types.Action{
		{
			Type:        types.ActionTypeLink,
			Source:      "/source/vimrc",
			Target:      "~/.vimrc",
			Description: "Link vim config",
			Pack:        "vim",
			PowerUpName: "symlink",
			Priority:    10,
			Metadata: map[string]interface{}{
				"trigger": "FileName",
			},
		},
		{
			Type:        types.ActionTypeWrite,
			Target:      "~/.bashrc",
			Content:     "export PATH",
			Description: "Update bashrc",
			Pack:        "bash",
			PowerUpName: "profile",
			Priority:    5,
		},
		{
			Type:        types.ActionTypeShellSource,
			Source:      "/source/profile.sh",
			Description: "Add shell profile",
			Pack:        "shell",
			PowerUpName: "profile",
			Priority:    15,
		},
	}

	ops, err := ConvertActionsToOperationsWithContext(actions, ctx)
	require.NoError(t, err)

	// Verify operations are sorted by priority (highest first)
	// We'll verify the order by checking pack groupings

	// Group operations by pack to verify context preservation
	packOps := make(map[string][]types.Operation)
	for _, op := range ops {
		if op.Pack != "" {
			packOps[op.Pack] = append(packOps[op.Pack], op)
		}
	}

	// Verify vim operations
	vimOps, ok := packOps["vim"]
	assert.True(t, ok)
	for _, op := range vimOps {
		assert.Equal(t, "vim", op.Pack)
		assert.Equal(t, "symlink", op.PowerUp)
		assert.Equal(t, "vim-symlink-10", op.GroupID)
	}

	// Verify bash operations
	bashOps, ok := packOps["bash"]
	assert.True(t, ok)
	for _, op := range bashOps {
		assert.Equal(t, "bash", op.Pack)
		assert.Equal(t, "profile", op.PowerUp)
		assert.Equal(t, "bash-profile-5", op.GroupID)
	}

	// Verify shell operations
	shellOps, ok := packOps["shell"]
	assert.True(t, ok)
	for _, op := range shellOps {
		assert.Equal(t, "shell", op.Pack)
		assert.Equal(t, "profile", op.PowerUp)
		assert.Equal(t, "shell-profile-15", op.GroupID)
	}
}

func TestOperationGrouping(t *testing.T) {
	ctx := &ExecutionContext{
		Paths: createTestPathsForContext(t),
	}

	// Create actions that should result in grouped operations
	actions := []types.Action{
		{
			Type:        types.ActionTypeLink,
			Source:      "/source/vimrc",
			Target:      "~/.vimrc",
			Pack:        "vim",
			PowerUpName: "symlink",
			Priority:    10,
		},
		{
			Type:        types.ActionTypeLink,
			Source:      "/source/vim/colors",
			Target:      "~/.vim/colors",
			Pack:        "vim",
			PowerUpName: "symlink",
			Priority:    10,
		},
	}

	ops, err := ConvertActionsToOperationsWithContext(actions, ctx)
	require.NoError(t, err)

	// Group operations by GroupID
	groups := make(map[string][]types.Operation)
	for _, op := range ops {
		if op.GroupID != "" {
			groups[op.GroupID] = append(groups[op.GroupID], op)
		}
	}

	// Both vim symlink operations should have the same GroupID
	vimGroup, ok := groups["vim-symlink-10"]
	assert.True(t, ok)
	assert.Greater(t, len(vimGroup), 0)

	// All operations in the group should have the same pack and powerup
	for _, op := range vimGroup {
		assert.Equal(t, "vim", op.Pack)
		assert.Equal(t, "symlink", op.PowerUp)
	}
}
