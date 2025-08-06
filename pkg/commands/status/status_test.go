package status

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestDisplayPackStatusAggregation tests the pack status aggregation rules
func TestDisplayPackStatusAggregation(t *testing.T) {
	tests := []struct {
		name           string
		files          []types.DisplayFile
		expectedStatus string
	}{
		{
			name:           "empty pack returns queue",
			files:          []types.DisplayFile{},
			expectedStatus: "queue",
		},
		{
			name: "all success returns success",
			files: []types.DisplayFile{
				{Path: ".vimrc", PowerUp: "symlink", Status: "success"},
				{Path: ".vim/", PowerUp: "symlink", Status: "success"},
			},
			expectedStatus: "success",
		},
		{
			name: "any error returns alert",
			files: []types.DisplayFile{
				{Path: ".vimrc", PowerUp: "symlink", Status: "success"},
				{Path: "install.sh", PowerUp: "install", Status: "error"},
			},
			expectedStatus: "alert",
		},
		{
			name: "all queue returns queue",
			files: []types.DisplayFile{
				{Path: ".vimrc", PowerUp: "symlink", Status: "queue"},
				{Path: ".vim/", PowerUp: "symlink", Status: "queue"},
			},
			expectedStatus: "queue",
		},
		{
			name: "mixed success and queue returns queue",
			files: []types.DisplayFile{
				{Path: ".vimrc", PowerUp: "symlink", Status: "success"},
				{Path: "install.sh", PowerUp: "install", Status: "queue"},
			},
			expectedStatus: "queue",
		},
		{
			name: "config files are ignored in aggregation",
			files: []types.DisplayFile{
				{Path: ".dodot.toml", PowerUp: "config", Status: "config"},
				{Path: ".vimrc", PowerUp: "symlink", Status: "success"},
			},
			expectedStatus: "success",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pack := &types.DisplayPack{
				Name:  "test-pack",
				Files: tt.files,
			}

			status := pack.GetPackStatus()
			assert.Equal(t, tt.expectedStatus, status)
		})
	}
}

// TestCreateDisplayResultFromOperations tests the transformation function
func TestCreateDisplayResultFromOperations(t *testing.T) {
	packs := []types.Pack{
		{
			Name: "vim",
			Path: "/home/user/dotfiles/vim",
		},
	}

	operations := []types.Operation{
		{
			Type:    types.OperationCreateSymlink,
			Source:  "/home/user/dotfiles/vim/.vimrc",
			Target:  "/home/user/.vimrc",
			PowerUp: "symlink",
			Pack:    "vim",
			Status:  types.StatusReady,
			TriggerInfo: &types.TriggerMatchInfo{
				OriginalPath: ".vimrc",
			},
		},
		{
			Type:    types.OperationExecute,
			Source:  "/home/user/dotfiles/vim/install.sh",
			PowerUp: "install",
			Pack:    "vim",
			Status:  types.StatusReady,
			TriggerInfo: &types.TriggerMatchInfo{
				TriggerName:  "override-rule",
				OriginalPath: "install.sh",
			},
		},
	}

	result := CreateDisplayResultFromOperations(operations, packs, "status")

	require.NotNil(t, result)
	assert.Equal(t, "status", result.Command)
	assert.Len(t, result.Packs, 1)

	pack := result.Packs[0]
	assert.Equal(t, "vim", pack.Name)
	assert.Len(t, pack.Files, 2)

	// Check first file (symlink)
	file1 := pack.Files[0]
	assert.Equal(t, ".vimrc", file1.Path)
	assert.Equal(t, "symlink", file1.PowerUp)
	assert.Equal(t, "queue", file1.Status)
	assert.Equal(t, "will be linked to target", file1.Message)
	assert.False(t, file1.IsOverride)

	// Check second file (install with override)
	file2 := pack.Files[1]
	assert.Equal(t, "*install.sh", file2.Path)
	assert.Equal(t, "install", file2.PowerUp)
	assert.Equal(t, "queue", file2.Status)
	assert.Equal(t, "to be executed", file2.Message)
	assert.True(t, file2.IsOverride)
}

// TestStatusPacksWithNewModel tests StatusPacks returns DisplayResult
func TestStatusPacksWithNewModel(t *testing.T) {
	t.Skip("Integration test - requires full environment setup")

	opts := StatusPacksOptions{
		DotfilesRoot: "/test/dotfiles",
		PackNames:    []string{"vim"},
	}

	result, err := StatusPacks(opts)

	require.NoError(t, err)
	require.NotNil(t, result)

	// Verify it returns DisplayResult
	assert.Equal(t, "status", result.Command)
	assert.IsType(t, &types.DisplayResult{}, result)
}
