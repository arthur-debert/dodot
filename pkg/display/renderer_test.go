package display

import (
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestRichRenderer_RenderFileResult(t *testing.T) {
	renderer := NewRichRenderer()

	tests := []struct {
		name     string
		file     FileResult
		contains []string
	}{
		{
			name: "successful symlink",
			file: FileResult{
				Action:  "Link",
				Path:    "~/.vimrc",
				Status:  types.StatusReady,
				Message: "Applied",
				PowerUp: "symlink",
			},
			contains: []string{"Link", "~/.vimrc", "Applied"},
		},
		{
			name: "skipped with output",
			file: FileResult{
				Action:  "Install",
				Path:    "~/Brewfile",
				Status:  types.StatusSkipped,
				Message: "Already processed",
				PowerUp: "homebrew",
				Output:  "brew output here",
			},
			contains: []string{"Install", "~/Brewfile", "Already processed", "[output]"},
		},
		{
			name: "error status",
			file: FileResult{
				Action:  "Execute",
				Path:    "~/scripts/install.sh",
				Status:  types.StatusError,
				Message: "Error: permission denied",
				PowerUp: "install",
			},
			contains: []string{"Execute", "~/scripts/install.sh", "Error: permission denied"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := renderer.RenderFileResult(tt.file)
			for _, expected := range tt.contains {
				assert.Contains(t, result, expected)
			}
		})
	}
}

func TestRichRenderer_RenderPackResult(t *testing.T) {
	renderer := NewRichRenderer()

	pack := PackResult{
		Name:        "vim",
		Description: "Vim configuration files",
		Status:      types.ExecutionStatusSuccess,
		Files: []FileResult{
			{
				Action:  "Link",
				Path:    "~/.vimrc",
				Status:  types.StatusReady,
				Message: "Applied",
				PowerUp: "symlink",
			},
			{
				Action:  "Link",
				Path:    "~/.vim",
				Status:  types.StatusSkipped,
				Message: "Already up to date",
				PowerUp: "symlink",
			},
		},
		TotalOperations:     2,
		CompletedOperations: 1,
		SkippedOperations:   1,
	}

	result := renderer.RenderPackResult(pack)

	// Check pack header
	assert.Contains(t, result, "vim")
	assert.Contains(t, result, "Vim configuration files")

	// Check files are rendered
	assert.Contains(t, result, "~/.vimrc")
	assert.Contains(t, result, "~/.vim")

	// Check summary
	assert.Contains(t, result, "1 completed")
	assert.Contains(t, result, "1 skipped")
}

func TestRichRenderer_RenderCommandResult(t *testing.T) {
	renderer := NewRichRenderer()

	result := CommandResult{
		Command: "deploy",
		DryRun:  true,
		Packs: []PackResult{
			{
				Name:   "vim",
				Status: types.ExecutionStatusSuccess,
				Files: []FileResult{
					{
						Action:  "Link",
						Path:    "~/.vimrc",
						Status:  types.StatusReady,
						Message: "Applied",
						PowerUp: "symlink",
					},
				},
				TotalOperations:     1,
				CompletedOperations: 1,
			},
		},
		Summary: Summary{
			TotalPacks:          1,
			TotalOperations:     1,
			CompletedOperations: 1,
			Duration:            5 * time.Second,
		},
	}

	output := renderer.RenderCommandResult(result)

	// Check header
	assert.Contains(t, output, "Deploy")
	assert.Contains(t, output, "dry run")

	// Check pack is rendered
	assert.Contains(t, output, "vim")

	// Check summary
	assert.Contains(t, output, "Summary")
	assert.Contains(t, output, "Total packs: 1")
	assert.Contains(t, output, "Completed: 1")
}

func TestRichRenderer_PadRight(t *testing.T) {
	renderer := NewRichRenderer()

	tests := []struct {
		name     string
		input    string
		width    int
		expected string
	}{
		{
			name:     "short string",
			input:    "test",
			width:    10,
			expected: "test      ",
		},
		{
			name:     "exact width",
			input:    "1234567890",
			width:    10,
			expected: "1234567890",
		},
		{
			name:     "long string truncated",
			input:    "this is a very long string",
			width:    10,
			expected: "this is a…",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := renderer.padRight(tt.input, tt.width)
			assert.Equal(t, tt.expected, result)
			// Unicode ellipsis counts as 3 bytes but 1 rune
			assert.LessOrEqual(t, len([]rune(result)), tt.width)
		})
	}
}

func TestPlainRenderer_RenderFileResult(t *testing.T) {
	renderer := NewPlainRenderer()

	file := FileResult{
		Action:  "Link",
		Path:    "~/.vimrc",
		Status:  types.StatusReady,
		Message: "Applied",
		PowerUp: "symlink",
	}

	result := renderer.RenderFileResult(file)

	assert.Contains(t, result, "[✓]")
	assert.Contains(t, result, "Link")
	assert.Contains(t, result, "~/.vimrc")
	assert.Contains(t, result, "Applied")
}

func TestPlainRenderer_RenderCommandResult(t *testing.T) {
	renderer := NewPlainRenderer()

	result := CommandResult{
		Command: "deploy",
		DryRun:  false,
		Packs: []PackResult{
			{
				Name: "vim",
				Files: []FileResult{
					{
						Action:  "Link",
						Path:    "~/.vimrc",
						Status:  types.StatusReady,
						Message: "Applied",
					},
				},
			},
		},
		Summary: Summary{
			TotalPacks:          1,
			TotalOperations:     1,
			CompletedOperations: 1,
		},
	}

	output := renderer.RenderCommandResult(result)

	// Plain output should be uppercase and contain key information
	assert.Contains(t, output, "DEPLOY")
	assert.Contains(t, output, "vim:")
	assert.Contains(t, output, "SUMMARY")
	assert.NotContains(t, output, "DRY RUN") // Not a dry run
}

func TestGetPackStatusIndicator(t *testing.T) {
	renderer := NewRichRenderer()

	tests := []struct {
		status   types.ExecutionStatus
		contains string
	}{
		{types.ExecutionStatusSuccess, "✓"},
		{types.ExecutionStatusError, "✗"},
		{types.ExecutionStatusPartial, "!"}, // WarningIndicator uses "!"
		{types.ExecutionStatusSkipped, "•"}, // InfoIndicator uses "•"
	}

	for _, tt := range tests {
		t.Run(string(tt.status), func(t *testing.T) {
			indicator := renderer.getPackStatusIndicator(tt.status)
			// The indicator is just the text without styling
			assert.Equal(t, tt.contains, indicator)
		})
	}
}

func TestGroupPacksByStatus(t *testing.T) {
	packs := []PackResult{
		{Name: "vim", Status: types.ExecutionStatusSuccess},
		{Name: "tmux", Status: types.ExecutionStatusSuccess},
		{Name: "zsh", Status: types.ExecutionStatusPartial},
		{Name: "git", Status: types.ExecutionStatusError},
	}

	groups := GroupPacksByStatus(packs)

	assert.Len(t, groups[types.ExecutionStatusSuccess], 2)
	assert.Contains(t, groups[types.ExecutionStatusSuccess], "vim")
	assert.Contains(t, groups[types.ExecutionStatusSuccess], "tmux")
	assert.Len(t, groups[types.ExecutionStatusPartial], 1)
	assert.Contains(t, groups[types.ExecutionStatusPartial], "zsh")
	assert.Len(t, groups[types.ExecutionStatusError], 1)
	assert.Contains(t, groups[types.ExecutionStatusError], "git")
}
