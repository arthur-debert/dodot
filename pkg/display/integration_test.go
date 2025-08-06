package display_test

import (
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/display"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestDisplayIntegration tests the complete flow from ExecutionContext to rendered output
func TestDisplayIntegration(t *testing.T) {
	// Create a realistic execution context
	ctx := createTestExecutionContext()

	// Convert to display format
	converter := display.NewConverter("/home/user")
	commandResult := converter.ConvertExecutionContext(ctx)

	// Verify conversion preserved all information
	assert.Equal(t, "deploy", commandResult.Command)
	assert.True(t, commandResult.DryRun)
	assert.Len(t, commandResult.Packs, 2)

	// Check vim pack
	vimPack := findPackByName(commandResult.Packs, "vim")
	require.NotNil(t, vimPack)
	assert.Equal(t, types.ExecutionStatusSuccess, vimPack.Status)
	assert.Len(t, vimPack.Files, 2)

	// Check that PowerUp grouping works
	groups := vimPack.GroupFilesByPowerUp()
	assert.Len(t, groups["symlink"], 2)

	// Check tmux pack
	tmuxPack := findPackByName(commandResult.Packs, "tmux")
	require.NotNil(t, tmuxPack)
	assert.Equal(t, types.ExecutionStatusPartial, tmuxPack.Status)

	// Render with rich renderer
	richRenderer := display.NewRichRenderer()
	richOutput := richRenderer.RenderCommandResult(commandResult)

	// Verify rich output contains expected elements
	assert.Contains(t, richOutput, "Deploy (dry run)")
	assert.Contains(t, richOutput, "vim")
	assert.Contains(t, richOutput, "tmux")
	assert.Contains(t, richOutput, "symlink")
	assert.Contains(t, richOutput, "~/.vimrc")
	assert.Contains(t, richOutput, "homebrew")
	assert.Contains(t, richOutput, "Summary")

	// Render with plain renderer
	plainRenderer := display.NewPlainRenderer()
	plainOutput := plainRenderer.RenderCommandResult(commandResult)

	// Verify plain output
	assert.Contains(t, plainOutput, "DEPLOY (DRY RUN)")
	assert.Contains(t, plainOutput, "vim:")
	assert.Contains(t, plainOutput, "tmux:")
	assert.Contains(t, plainOutput, " : ") // Check for proper column separators
}

// TestFileStatusConversion tests converting FileStatus to display format
func TestFileStatusConversion(t *testing.T) {
	// Create file statuses from status checking
	fileStatuses := []*types.FileStatus{
		{
			Path:        "/home/user/.vimrc",
			PowerUp:     "symlink",
			Status:      types.StatusSkipped,
			Message:     "Symlink already exists with correct target",
			LastApplied: time.Now().Add(-7 * 24 * time.Hour),
			Metadata: map[string]interface{}{
				"target":        "/dotfiles/vim/.vimrc",
				"link_valid":    true,
				"target_exists": true,
			},
		},
		{
			Path:    "/home/user/.local/share/dodot/deployed/homebrew/tmux",
			PowerUp: "homebrew",
			Status:  types.StatusReady,
			Message: "Brewfile has changed (checksum mismatch)",
			Metadata: map[string]interface{}{
				"current_checksum": "abc123",
				"stored_checksum":  "def456",
			},
		},
	}

	converter := display.NewConverter("/home/user")

	for _, fs := range fileStatuses {
		fileResult := converter.ConvertFileStatus(fs)

		// Verify conversion
		switch fs.PowerUp {
		case "symlink":
			assert.Equal(t, "Link", fileResult.Action)
			assert.Equal(t, "~/.vimrc", fileResult.Path)
			assert.Equal(t, types.StatusSkipped, fileResult.Status)
			assert.Equal(t, "Symlink already exists with correct target", fileResult.Message)
			assert.False(t, fileResult.LastApplied.IsZero())
		case "homebrew":
			assert.Equal(t, "Install", fileResult.Action)
			assert.Contains(t, fileResult.Path, "homebrew/tmux")
			assert.Equal(t, types.StatusReady, fileResult.Status)
			assert.Equal(t, "Brewfile has changed (checksum mismatch)", fileResult.Message)
		}
	}
}

// Helper functions

func createTestExecutionContext() *types.ExecutionContext {
	now := time.Now()

	return &types.ExecutionContext{
		Command:   "deploy",
		DryRun:    true,
		StartTime: now.Add(-30 * time.Second),
		EndTime:   now,
		PackResults: map[string]*types.PackExecutionResult{
			"vim": {
				Pack: &types.Pack{
					Name: "vim",
					Metadata: map[string]interface{}{
						"description": "Vim configuration and plugins",
					},
				},
				Status:              types.ExecutionStatusSuccess,
				StartTime:           now.Add(-25 * time.Second),
				EndTime:             now.Add(-20 * time.Second),
				TotalOperations:     2,
				CompletedOperations: 1,
				SkippedOperations:   1,
				Operations: []*types.OperationResult{
					{
						Operation: &types.Operation{
							Type:        types.OperationCreateSymlink,
							Source:      "/dotfiles/vim/.vimrc",
							Target:      "/home/user/.vimrc",
							Description: "Link .vimrc",
							PowerUp:     "symlink",
							Pack:        "vim",
							GroupID:     "vim-config",
						},
						Status:    types.StatusReady,
						StartTime: now.Add(-25 * time.Second),
						EndTime:   now.Add(-24 * time.Second),
					},
					{
						Operation: &types.Operation{
							Type:        types.OperationCreateSymlink,
							Source:      "/dotfiles/vim/.vim",
							Target:      "/home/user/.vim",
							Description: "Link .vim directory",
							PowerUp:     "symlink",
							Pack:        "vim",
							GroupID:     "vim-config",
						},
						Status:    types.StatusSkipped,
						StartTime: now.Add(-24 * time.Second),
						EndTime:   now.Add(-23 * time.Second),
					},
				},
			},
			"tmux": {
				Pack: &types.Pack{
					Name: "tmux",
				},
				Status:              types.ExecutionStatusPartial,
				StartTime:           now.Add(-20 * time.Second),
				EndTime:             now.Add(-10 * time.Second),
				TotalOperations:     2,
				CompletedOperations: 1,
				FailedOperations:    1,
				Operations: []*types.OperationResult{
					{
						Operation: &types.Operation{
							Type:        types.OperationCreateSymlink,
							Source:      "/dotfiles/tmux/.tmux.conf",
							Target:      "/home/user/.tmux.conf",
							Description: "Link .tmux.conf",
							PowerUp:     "symlink",
							Pack:        "tmux",
						},
						Status:    types.StatusReady,
						StartTime: now.Add(-20 * time.Second),
						EndTime:   now.Add(-19 * time.Second),
					},
					{
						Operation: &types.Operation{
							Type:        types.OperationExecute,
							Source:      "/dotfiles/tmux/Brewfile",
							Command:     "brew",
							Args:        []string{"bundle", "--file", "/dotfiles/tmux/Brewfile"},
							Description: "Install tmux packages",
							PowerUp:     "homebrew",
							Pack:        "tmux",
						},
						Status:    types.StatusError,
						Error:     assert.AnError,
						StartTime: now.Add(-19 * time.Second),
						EndTime:   now.Add(-10 * time.Second),
						Output:    "Error: brew command not found",
					},
				},
			},
		},
		TotalOperations:     4,
		CompletedOperations: 2,
		FailedOperations:    1,
		SkippedOperations:   1,
	}
}

func findPackByName(packs []display.PackResult, name string) *display.PackResult {
	for i := range packs {
		if packs[i].Name == name {
			return &packs[i]
		}
	}
	return nil
}
