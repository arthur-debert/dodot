package display

import (
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestConverter_ConvertOperationWithOverride(t *testing.T) {
	converter := NewConverter("/home/user")

	// Create an operation that was overridden
	or := &types.OperationResult{
		Operation: &types.Operation{
			Type:    types.OperationExecute,
			PowerUp: "install",
			Source:  "/home/user/dotfiles/vim/runme.sh",
			Target:  "/home/user/.local/bin/runme.sh",
			TriggerInfo: &types.TriggerMatchInfo{
				TriggerName:  "override-rule",
				OriginalPath: "runme.sh",
				Priority:     100,
			},
		},
		Status: types.StatusReady,
	}

	// Convert
	result := converter.ConvertOperationResult(or)

	// Path should have asterisk prefix and show the original filename
	assert.Equal(t, "*runme.sh", result.Path)
	assert.Equal(t, "install", result.PowerUp)
}

func TestConverter_ConvertPackWithConfig(t *testing.T) {
	converter := NewConverter("/home/user")

	// Create a pack with config
	pack := &types.Pack{
		Name: "vim",
		Path: "/home/user/dotfiles/vim",
		Config: types.PackConfig{
			Override: []types.OverrideRule{
				{Path: "vimrc", Powerup: "symlink"},
			},
		},
	}

	// Create pack execution result
	per := &types.PackExecutionResult{
		Pack:   pack,
		Status: types.ExecutionStatusSuccess,
		Operations: []*types.OperationResult{
			{
				Operation: &types.Operation{
					PowerUp: "symlink",
					Target:  "/home/user/.vimrc",
				},
				Status: types.StatusReady,
			},
		},
	}

	// Convert
	result := converter.ConvertPackExecutionResult("vim", per)

	// Should have 2 files: config + operation
	assert.Len(t, result.Files, 2)

	// First file should be config
	configFile := result.Files[0]
	assert.Equal(t, "config", configFile.PowerUp)
	assert.Equal(t, ".dodot.toml", configFile.Path)
	assert.Equal(t, "dodot config file found", configFile.Message)
	assert.Equal(t, types.StatusReady, configFile.Status)
}

func TestConverter_ConvertExecutionContext(t *testing.T) {
	converter := NewConverter("/home/user")

	// Create a test execution context
	ctx := &types.ExecutionContext{
		Command:   "deploy",
		DryRun:    true,
		StartTime: time.Now().Add(-5 * time.Minute),
		EndTime:   time.Now(),
		PackResults: map[string]*types.PackExecutionResult{
			"vim": {
				Pack: &types.Pack{
					Name: "vim",
					Metadata: map[string]interface{}{
						"description": "Vim configuration",
					},
				},
				Status:              types.ExecutionStatusSuccess,
				TotalOperations:     2,
				CompletedOperations: 1,
				SkippedOperations:   1,
				Operations: []*types.OperationResult{
					{
						Operation: &types.Operation{
							Type:    types.OperationCreateSymlink,
							Source:  "/dotfiles/vim/.vimrc",
							Target:  "/home/user/.vimrc",
							PowerUp: "symlink",
							Pack:    "vim",
						},
						Status:  types.StatusReady,
						EndTime: time.Now(),
					},
					{
						Operation: &types.Operation{
							Type:    types.OperationCreateSymlink,
							Source:  "/dotfiles/vim/.vim",
							Target:  "/home/user/.vim",
							PowerUp: "symlink",
							Pack:    "vim",
						},
						Status: types.StatusSkipped,
					},
				},
			},
		},
	}

	result := converter.ConvertExecutionContext(ctx)

	assert.Equal(t, "deploy", result.Command)
	assert.True(t, result.DryRun)
	assert.Len(t, result.Packs, 1)

	// Check pack conversion
	pack := result.Packs[0]
	assert.Equal(t, "vim", pack.Name)
	assert.Equal(t, "Vim configuration", pack.Description)
	assert.Equal(t, types.ExecutionStatusSuccess, pack.Status)
	assert.Equal(t, 2, pack.TotalOperations)
	assert.Equal(t, 1, pack.CompletedOperations)
	assert.Equal(t, 1, pack.SkippedOperations)

	// Check file conversions
	assert.Len(t, pack.Files, 2)

	file1 := pack.Files[0]
	assert.Equal(t, "Link", file1.Action)
	assert.Equal(t, "~/.vimrc", file1.Path)
	assert.Equal(t, types.StatusReady, file1.Status)
	assert.Equal(t, "will be linked to target", file1.Message)
	assert.True(t, file1.IsNewChange)

	file2 := pack.Files[1]
	assert.Equal(t, "Link", file2.Action)
	assert.Equal(t, "~/.vim", file2.Path)
	assert.Equal(t, types.StatusSkipped, file2.Status)
	assert.Equal(t, "linked to target", file2.Message)
	assert.False(t, file2.IsNewChange)

	// Check summary
	assert.Equal(t, 1, result.Summary.TotalPacks)
	assert.Equal(t, 2, result.Summary.TotalOperations)
	assert.Equal(t, 1, result.Summary.CompletedOperations)
	assert.Equal(t, 1, result.Summary.SkippedOperations)
	assert.Equal(t, 1, result.Summary.SuccessfulPacks)
}

func TestConverter_GetActionVerb(t *testing.T) {
	converter := NewConverter("")

	tests := []struct {
		name     string
		op       *types.Operation
		expected string
	}{
		{
			name: "symlink powerup",
			op: &types.Operation{
				PowerUp: "symlink",
			},
			expected: "Link",
		},
		{
			name: "shell_profile powerup",
			op: &types.Operation{
				PowerUp: "shell_profile",
			},
			expected: "Source",
		},
		{
			name: "add_path powerup",
			op: &types.Operation{
				PowerUp: "add_path",
			},
			expected: "Add to PATH",
		},
		{
			name: "homebrew powerup",
			op: &types.Operation{
				PowerUp: "homebrew",
			},
			expected: "Install",
		},
		{
			name: "unknown powerup falls back to operation type",
			op: &types.Operation{
				PowerUp: "unknown",
				Type:    types.OperationExecute,
			},
			expected: "Execute",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := converter.getActionVerb(tt.op)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestConverter_MakePathRelative(t *testing.T) {
	converter := NewConverter("/home/user")

	tests := []struct {
		name     string
		path     string
		expected string
	}{
		{
			name:     "home directory path",
			path:     "/home/user/.vimrc",
			expected: "~/.vimrc",
		},
		{
			name:     "subdirectory of home",
			path:     "/home/user/.config/nvim/init.vim",
			expected: "~/.config/nvim/init.vim",
		},
		{
			name:     "path outside home",
			path:     "/etc/hosts",
			expected: "/etc/hosts",
		},
		{
			name:     "relative path with ./",
			path:     "./config/file.conf",
			expected: "config/file.conf",
		},
		{
			name:     "empty path",
			path:     "",
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := converter.makePathRelative(tt.path)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestConverter_GetStatusMessage(t *testing.T) {
	converter := NewConverter("")

	tests := []struct {
		name     string
		or       *types.OperationResult
		expected string
	}{
		{
			name: "ready symlink",
			or: &types.OperationResult{
				Status: types.StatusReady,
				Operation: &types.Operation{
					PowerUp: "symlink",
				},
			},
			expected: "will be linked to target",
		},
		{
			name: "skipped homebrew",
			or: &types.OperationResult{
				Status: types.StatusSkipped,
				Operation: &types.Operation{
					PowerUp: "homebrew",
				},
			},
			expected: "executed",
		},
		{
			name: "skipped symlink",
			or: &types.OperationResult{
				Status: types.StatusSkipped,
				Operation: &types.Operation{
					PowerUp: "symlink",
				},
			},
			expected: "linked to target",
		},
		{
			name: "conflict with error",
			or: &types.OperationResult{
				Status: types.StatusConflict,
				Operation: &types.Operation{
					PowerUp: "symlink",
				},
				Error: assert.AnError,
			},
			expected: "Conflict: assert.AnError general error for testing",
		},
		{
			name: "error with message",
			or: &types.OperationResult{
				Status: types.StatusError,
				Operation: &types.Operation{
					PowerUp: "symlink",
				},
				Error: assert.AnError,
			},
			expected: "Error: assert.AnError general error for testing",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := converter.getStatusMessage(tt.or)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestConverter_ConvertFileStatus(t *testing.T) {
	converter := NewConverter("/home/user")

	fs := &types.FileStatus{
		Path:        "/home/user/.vimrc",
		PowerUp:     "symlink",
		Status:      types.StatusSkipped,
		Message:     "Symlink already exists",
		LastApplied: time.Now().Add(-24 * time.Hour),
		Metadata: map[string]interface{}{
			"target": "/dotfiles/vim/.vimrc",
		},
	}

	result := converter.ConvertFileStatus(fs)

	assert.Equal(t, "Link", result.Action)
	assert.Equal(t, "~/.vimrc", result.Path)
	assert.Equal(t, types.StatusSkipped, result.Status)
	assert.Equal(t, "Symlink already exists", result.Message)
	assert.Equal(t, "symlink", result.PowerUp)
	assert.False(t, result.LastApplied.IsZero())
	assert.Equal(t, "/dotfiles/vim/.vimrc", result.Metadata["target"])
}

func TestCommandResult_GetOverallStatus(t *testing.T) {
	tests := []struct {
		name     string
		result   CommandResult
		expected types.ExecutionStatus
	}{
		{
			name: "all operations failed",
			result: CommandResult{
				Summary: Summary{
					TotalOperations:  5,
					FailedOperations: 5,
				},
			},
			expected: types.ExecutionStatusError,
		},
		{
			name: "all operations skipped",
			result: CommandResult{
				Summary: Summary{
					TotalOperations:   5,
					SkippedOperations: 5,
				},
			},
			expected: types.ExecutionStatusSkipped,
		},
		{
			name: "partial success",
			result: CommandResult{
				Summary: Summary{
					TotalOperations:     5,
					CompletedOperations: 3,
					FailedOperations:    2,
				},
			},
			expected: types.ExecutionStatusPartial,
		},
		{
			name: "full success",
			result: CommandResult{
				Summary: Summary{
					TotalOperations:     5,
					CompletedOperations: 5,
				},
			},
			expected: types.ExecutionStatusSuccess,
		},
		{
			name: "no operations",
			result: CommandResult{
				Summary: Summary{
					TotalOperations: 0,
				},
			},
			expected: types.ExecutionStatusPending,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.result.GetOverallStatus()
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestPackResult_GroupFilesByPowerUp(t *testing.T) {
	pack := PackResult{
		Files: []FileResult{
			{PowerUp: "symlink", Path: "file1"},
			{PowerUp: "symlink", Path: "file2"},
			{PowerUp: "shell_profile", Path: "script1"},
			{PowerUp: "", Path: "unknown1"},
		},
	}

	groups := pack.GroupFilesByPowerUp()

	require.Len(t, groups, 3)
	assert.Len(t, groups["symlink"], 2)
	assert.Len(t, groups["shell_profile"], 1)
	assert.Len(t, groups["unknown"], 1)
}
