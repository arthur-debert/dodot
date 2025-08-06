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

	// Find files by path (order is not guaranteed)
	var vimrcFile, installFile *types.DisplayFile
	for i := range pack.Files {
		switch pack.Files[i].Path {
		case ".vimrc":
			vimrcFile = &pack.Files[i]
		case "*install.sh":
			installFile = &pack.Files[i]
		}
	}

	// Check symlink file
	require.NotNil(t, vimrcFile, ".vimrc file should exist")
	assert.Equal(t, "symlink", vimrcFile.PowerUp)
	assert.Equal(t, "queue", vimrcFile.Status)
	assert.Equal(t, "will be linked to target", vimrcFile.Message)
	assert.False(t, vimrcFile.IsOverride)

	// Check install file with override
	require.NotNil(t, installFile, "install.sh file should exist")
	assert.Equal(t, "install", installFile.PowerUp)
	assert.Equal(t, "queue", installFile.Status)
	assert.Equal(t, "to be executed", installFile.Message)
	assert.True(t, installFile.IsOverride)
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
