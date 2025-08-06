package status

import (
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestCreateDisplayResultFromOperationsEdgeCases tests edge cases
func TestCreateDisplayResultFromOperationsEdgeCases(t *testing.T) {
	tests := []struct {
		name       string
		operations []types.Operation
		packs      []types.Pack
		command    string
		validate   func(t *testing.T, result *types.DisplayResult)
	}{
		{
			name:       "empty operations and packs",
			operations: []types.Operation{},
			packs:      []types.Pack{},
			command:    "status",
			validate: func(t *testing.T, result *types.DisplayResult) {
				assert.Equal(t, "status", result.Command)
				assert.Empty(t, result.Packs)
				assert.False(t, result.DryRun)
			},
		},
		{
			name:       "packs without operations",
			operations: []types.Operation{},
			packs: []types.Pack{
				{Name: "vim", Path: "/dotfiles/vim"},
				{Name: "zsh", Path: "/dotfiles/zsh"},
			},
			command: "deploy",
			validate: func(t *testing.T, result *types.DisplayResult) {
				assert.Len(t, result.Packs, 2)
				for _, pack := range result.Packs {
					assert.Empty(t, pack.Files)
					assert.Equal(t, "queue", pack.Status)
				}
			},
		},
		{
			name: "operations with missing trigger info",
			operations: []types.Operation{
				{
					Type:        types.OperationCreateSymlink,
					Source:      "/dotfiles/vim/.vimrc",
					Target:      "/home/.vimrc",
					PowerUp:     "symlink",
					Pack:        "vim",
					Status:      types.StatusReady,
					TriggerInfo: nil, // Missing trigger info
				},
			},
			packs: []types.Pack{
				{Name: "vim", Path: "/dotfiles/vim"},
			},
			command: "status",
			validate: func(t *testing.T, result *types.DisplayResult) {
				require.Len(t, result.Packs, 1)
				require.Len(t, result.Packs[0].Files, 1)

				file := result.Packs[0].Files[0]
				assert.Equal(t, ".vimrc", file.Path) // Should extract from source
			},
		},
		{
			name: "operations with different statuses",
			operations: []types.Operation{
				{
					Type:    types.OperationCreateSymlink,
					Source:  "/dotfiles/vim/.vimrc",
					PowerUp: "symlink",
					Pack:    "vim",
					Status:  types.StatusReady,
					TriggerInfo: &types.TriggerMatchInfo{
						OriginalPath: ".vimrc",
					},
				},
				{
					Type:        types.OperationExecute,
					Source:      "/dotfiles/vim/install.sh",
					PowerUp:     "install",
					Pack:        "vim",
					Status:      types.StatusError,
					Description: "Permission denied",
					TriggerInfo: &types.TriggerMatchInfo{
						OriginalPath: "install.sh",
					},
				},
				{
					Type:    types.OperationCreateSymlink,
					Source:  "/dotfiles/vim/.vim",
					PowerUp: "symlink",
					Pack:    "vim",
					Status:  types.StatusSkipped,
					TriggerInfo: &types.TriggerMatchInfo{
						OriginalPath: ".vim",
					},
				},
			},
			packs: []types.Pack{
				{Name: "vim", Path: "/dotfiles/vim"},
			},
			command: "status",
			validate: func(t *testing.T, result *types.DisplayResult) {
				require.Len(t, result.Packs, 1)
				pack := result.Packs[0]
				assert.Equal(t, "alert", pack.Status) // Has error
				assert.Len(t, pack.Files, 3)

				// Check each file status
				fileMap := make(map[string]*types.DisplayFile)
				for i := range pack.Files {
					fileMap[pack.Files[i].Path] = &pack.Files[i]
				}

				assert.Equal(t, "queue", fileMap[".vimrc"].Status)
				assert.Equal(t, "error", fileMap["install.sh"].Status)
				assert.Equal(t, "Permission denied", fileMap["install.sh"].Message)
				assert.Equal(t, "success", fileMap[".vim"].Status)
			},
		},
		{
			name: "multiple operations for same file",
			operations: []types.Operation{
				{
					Type:    types.OperationCreateSymlink,
					Source:  "/dotfiles/vim/.vimrc",
					PowerUp: "symlink",
					Pack:    "vim",
					Status:  types.StatusReady,
					TriggerInfo: &types.TriggerMatchInfo{
						OriginalPath: ".vimrc",
					},
				},
				{
					Type:    types.OperationExecute,
					Source:  "/dotfiles/vim/.vimrc", // Same file, different powerup
					PowerUp: "install",
					Pack:    "vim",
					Status:  types.StatusReady,
					TriggerInfo: &types.TriggerMatchInfo{
						OriginalPath: ".vimrc",
					},
				},
			},
			packs: []types.Pack{
				{Name: "vim", Path: "/dotfiles/vim"},
			},
			command: "status",
			validate: func(t *testing.T, result *types.DisplayResult) {
				require.Len(t, result.Packs, 1)
				require.Len(t, result.Packs[0].Files, 1)

				file := result.Packs[0].Files[0]
				assert.Equal(t, "error", file.Status)
				assert.Equal(t, "Multiple power-ups for same file", file.Message)
			},
		},
		{
			name: "operation with conflict status",
			operations: []types.Operation{
				{
					Type:    types.OperationCreateSymlink,
					Source:  "/dotfiles/vim/.vimrc",
					PowerUp: "symlink",
					Pack:    "vim",
					Status:  types.StatusConflict,
					TriggerInfo: &types.TriggerMatchInfo{
						OriginalPath: ".vimrc",
					},
				},
			},
			packs: []types.Pack{
				{Name: "vim", Path: "/dotfiles/vim"},
			},
			command: "status",
			validate: func(t *testing.T, result *types.DisplayResult) {
				require.Len(t, result.Packs, 1)
				require.Len(t, result.Packs[0].Files, 1)

				file := result.Packs[0].Files[0]
				assert.Equal(t, "error", file.Status)
				assert.Equal(t, "Conflict detected", file.Message)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := CreateDisplayResultFromOperations(tt.operations, tt.packs, tt.command)
			require.NotNil(t, result)
			tt.validate(t, result)
		})
	}
}

// TestConvertOperationToDisplayFile tests the conversion function
func TestConvertOperationToDisplayFile(t *testing.T) {
	tests := []struct {
		name     string
		op       types.Operation
		filePath string
		expected types.DisplayFile
	}{
		{
			name: "ready status with override",
			op: types.Operation{
				PowerUp: "install",
				Status:  types.StatusReady,
				TriggerInfo: &types.TriggerMatchInfo{
					TriggerName: "override-rule",
				},
			},
			filePath: "install.sh",
			expected: types.DisplayFile{
				Path:       "*install.sh",
				PowerUp:    "install",
				Status:     "queue",
				Message:    "to be executed",
				IsOverride: true,
			},
		},
		{
			name: "skipped status",
			op: types.Operation{
				PowerUp: "symlink",
				Status:  types.StatusSkipped,
			},
			filePath: ".vimrc",
			expected: types.DisplayFile{
				Path:    ".vimrc",
				PowerUp: "symlink",
				Status:  "success",
				Message: "linked to target",
			},
		},
		{
			name: "error status with description",
			op: types.Operation{
				PowerUp:     "homebrew",
				Status:      types.StatusError,
				Description: "brew not found",
			},
			filePath: "Brewfile",
			expected: types.DisplayFile{
				Path:    "Brewfile",
				PowerUp: "homebrew",
				Status:  "error",
				Message: "brew not found",
			},
		},
		{
			name: "unknown status",
			op: types.Operation{
				PowerUp: "template",
				Status:  "unknown", // Unknown status
			},
			filePath: "config.tmpl",
			expected: types.DisplayFile{
				Path:    "config.tmpl",
				PowerUp: "template",
				Status:  "queue",
				Message: "To be processed",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := convertOperationToDisplayFile(tt.op, tt.filePath)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// TestGetRelativeFilePath tests path extraction
func TestGetRelativeFilePath(t *testing.T) {
	tests := []struct {
		name     string
		op       types.Operation
		packPath string
		expected string
	}{
		{
			name: "with trigger info",
			op: types.Operation{
				TriggerInfo: &types.TriggerMatchInfo{
					OriginalPath: ".vimrc",
				},
			},
			packPath: "/dotfiles/vim",
			expected: ".vimrc",
		},
		{
			name: "from source - relative path",
			op: types.Operation{
				Source: "/dotfiles/vim/.vimrc",
			},
			packPath: "/dotfiles/vim",
			expected: ".vimrc",
		},
		{
			name: "from source - nested path",
			op: types.Operation{
				Source: "/dotfiles/vim/.vim/colors/theme.vim",
			},
			packPath: "/dotfiles/vim",
			expected: ".vim/colors/theme.vim",
		},
		{
			name: "from source - fallback to basename",
			op: types.Operation{
				Source: "/other/path/.vimrc",
			},
			packPath: "/dotfiles/vim",
			expected: ".vimrc",
		},
		{
			name: "from target when no source",
			op: types.Operation{
				Target: "/home/.bashrc",
			},
			packPath: "/dotfiles/bash",
			expected: ".bashrc",
		},
		{
			name:     "no source or target",
			op:       types.Operation{},
			packPath: "/dotfiles/vim",
			expected: "unknown",
		},
		{
			name: "source is pack root",
			op: types.Operation{
				Source: "/dotfiles/vim",
			},
			packPath: "/dotfiles/vim",
			expected: "vim", // Fallback to basename
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := getRelativeFilePath(tt.op, tt.packPath)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// TestGetVerbForPowerUp tests verb generation
func TestGetVerbForPowerUp(t *testing.T) {
	tests := []struct {
		powerUp  string
		past     bool
		expected string
	}{
		// Known power-ups - past tense
		{"symlink", true, "linked to target"},
		{"shell_profile", true, "included in shell"},
		{"homebrew", true, "executed"},
		{"add_path", true, "added to $PATH"},
		{"install", true, "executed"},
		{"template", true, "generated from template"},
		{"config", true, "found"},

		// Known power-ups - future tense
		{"symlink", false, "will be linked to target"},
		{"shell_profile", false, "to be included in shell"},
		{"homebrew", false, "to be installed"},
		{"add_path", false, "to be added to $PATH"},
		{"install", false, "to be executed"},
		{"template", false, "to be generated"},
		{"config", false, "found"},

		// Unknown power-up
		{"custom", true, "processed"},
		{"custom", false, "to be processed"},
	}

	for _, tt := range tests {
		name := tt.powerUp
		if tt.past {
			name += "_past"
		} else {
			name += "_future"
		}

		t.Run(name, func(t *testing.T) {
			result := getVerbForPowerUp(tt.powerUp, tt.past)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// TestCreateDisplayResultTimestamp tests timestamp is set
func TestCreateDisplayResultTimestamp(t *testing.T) {
	before := time.Now()

	result := CreateDisplayResultFromOperations(
		[]types.Operation{},
		[]types.Pack{},
		"test",
	)

	after := time.Now()

	assert.True(t, result.Timestamp.After(before) || result.Timestamp.Equal(before))
	assert.True(t, result.Timestamp.Before(after) || result.Timestamp.Equal(after))
}

// TestPackWithConfigAndIgnore tests special file handling
func TestPackWithConfigAndIgnore(t *testing.T) {
	// Mock the config.FileExists function for this test
	// In real code, this would check actual files

	packs := []types.Pack{
		{Name: "vim", Path: "/dotfiles/vim"},
		{Name: "ignored", Path: "/dotfiles/ignored"},
	}

	// No operations for these packs
	operations := []types.Operation{}

	result := CreateDisplayResultFromOperations(operations, packs, "status")

	require.Len(t, result.Packs, 2)

	// Both packs should exist even without operations
	packMap := make(map[string]*types.DisplayPack)
	for i := range result.Packs {
		packMap[result.Packs[i].Name] = &result.Packs[i]
	}

	assert.Contains(t, packMap, "vim")
	assert.Contains(t, packMap, "ignored")
}
