package display_test

import (
	"bytes"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/display"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestTextRenderer_Render(t *testing.T) {
	tests := []struct {
		name        string
		result      *types.DisplayResult
		expected    []string // Lines that should appear in output
		notExpected []string // Lines that should NOT appear
	}{
		{
			name: "empty result with no packs",
			result: &types.DisplayResult{
				Command:   "link",
				Packs:     []types.DisplayPack{},
				Timestamp: time.Now(),
			},
			expected: []string{
				"link",
				"No packs to process",
			},
		},
		{
			name: "dry run mode indicator",
			result: &types.DisplayResult{
				Command:   "provision",
				DryRun:    true,
				Packs:     []types.DisplayPack{},
				Timestamp: time.Now(),
			},
			expected: []string{
				"provision (dry run)",
				"No packs to process",
			},
		},
		{
			name: "pack with successful files",
			result: &types.DisplayResult{
				Command: "link",
				Packs: []types.DisplayPack{
					{
						Name:   "vim",
						Status: "success",
						Files: []types.DisplayFile{
							{
								Handler: "symlink",
								Path:    "vimrc",
								Status:  "success",
								Message: "linked to $HOME/.vimrc",
							},
							{
								Handler: "symlink",
								Path:    "vim/colors/monokai.vim",
								Status:  "success",
								Message: "linked to $HOME/monokai.vim",
							},
						},
					},
				},
			},
			expected: []string{
				"link",
				"vim [status=success]:",
				"symlink",
				"vimrc",
				"vim/colors/monokai.vim",
				"linked to $HOME/.vimrc [status=success]",
				"linked to $HOME/monokai.vim [status=success]",
			},
		},
		{
			name: "pack with error status",
			result: &types.DisplayResult{
				Command: "provision",
				Packs: []types.DisplayPack{
					{
						Name:   "tools",
						Status: "alert",
						Files: []types.DisplayFile{
							{
								Handler: "provision",
								Path:    "install.sh",
								Status:  "error",
								Message: "installation failed",
							},
						},
					},
				},
			},
			expected: []string{
				"provision",
				"tools [status=alert]:",
				"provision",
				"install.sh",
				"installation failed [status=error]",
			},
		},
		{
			name: "mixed status pack",
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "shell",
						Status: "alert",
						Files: []types.DisplayFile{
							{
								Handler: "symlink",
								Path:    "bashrc",
								Status:  "success",
								Message: "linked to $HOME/.bashrc",
							},
							{
								Handler: "shell",
								Path:    "aliases",
								Status:  "queue",
								Message: "to be executed",
							},
						},
					},
				},
			},
			expected: []string{
				"status",
				"shell [status=alert]:",
				"symlink",
				"bashrc",
				"linked to $HOME/.bashrc [status=success]",
				"shell",
				"aliases",
				"to be executed [status=queue]",
			},
		},
		{
			name: "ignored pack",
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:      "temp",
						Status:    "ignored",
						IsIgnored: true,
					},
				},
			},
			expected: []string{
				"status",
				"temp [status=ignored] [ignored]:",
				".dodotignore : dodot is ignoring this dir",
			},
		},
		{
			name: "pack with config file",
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:      "neovim",
						Status:    "success",
						HasConfig: true,
						Files: []types.DisplayFile{
							{
								Handler: "config",
								Path:    ".dodot.toml",
								Status:  "config",
								Message: "dodot config file found",
							},
						},
					},
				},
			},
			expected: []string{
				"status",
				"neovim [status=success] [config]:",
				"config",
				".dodot.toml",
				"dodot config file found [status=config]",
			},
		},
		{
			name: "file with override marker",
			result: &types.DisplayResult{
				Command: "link",
				Packs: []types.DisplayPack{
					{
						Name:   "vim",
						Status: "success",
						Files: []types.DisplayFile{
							{
								Handler:    "provision",
								Path:       "setup.sh",
								Status:     "queue",
								Message:    "to be executed",
								IsOverride: true,
							},
						},
					},
				},
			},
			expected: []string{
				"link",
				"vim [status=success]:",
				"provision",
				"*setup.sh", // Override marker
				"to be executed [status=queue]",
			},
		},
		{
			name: "file with execution timestamp",
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "vim",
						Status: "success",
						Files: []types.DisplayFile{
							{
								Handler:      "symlink",
								Path:         ".vimrc",
								Status:       "success",
								Message:      "linked to $HOME/.vimrc",
								LastExecuted: func() *time.Time { t := time.Date(2024, 1, 15, 0, 0, 0, 0, time.UTC); return &t }(),
							},
						},
					},
				},
			},
			expected: []string{
				"status",
				"vim [status=success]:",
				"symlink",
				".vimrc",
				"linked to $HOME/.vimrc [status=success] [executed=2024-01-15]",
			},
		},
		{
			name: "empty pack with no files",
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "empty-pack",
						Status: "success",
						Files:  []types.DisplayFile{},
					},
				},
			},
			expected: []string{
				"status",
				"empty-pack [status=success]:",
				"(no files)",
			},
		},
		{
			name: "multiple packs sorted alphabetically",
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "zsh",
						Status: "success",
						Files: []types.DisplayFile{
							{Handler: "symlink", Path: ".zshrc", Status: "success", Message: "linked"},
						},
					},
					{
						Name:   "bash",
						Status: "success",
						Files: []types.DisplayFile{
							{Handler: "symlink", Path: ".bashrc", Status: "success", Message: "linked"},
						},
					},
				},
			},
			expected: []string{
				"status",
				"bash [status=success]:", // Should come first alphabetically
				".bashrc",
				"zsh [status=success]:", // Should come second
				".zshrc",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var buf bytes.Buffer
			renderer := display.NewTextRenderer(&buf)

			err := renderer.Render(tt.result)
			require.NoError(t, err)

			output := buf.String()

			// Check that all expected strings are present
			for _, expected := range tt.expected {
				assert.Contains(t, output, expected,
					"Expected output to contain '%s', but got:\n%s", expected, output)
			}

			// Check that notExpected strings are NOT present
			for _, notExpected := range tt.notExpected {
				assert.NotContains(t, output, notExpected,
					"Expected output NOT to contain '%s', but got:\n%s", notExpected, output)
			}
		})
	}
}

func TestTextRenderer_RenderExecutionContext(t *testing.T) {
	// Create a sample execution context
	ctx := types.NewExecutionContext("link", false)

	// Add a pack result
	pack := &types.Pack{
		Name: "test-pack",
		Path: "/path/to/test-pack",
	}
	packResult := types.NewPackExecutionResult(pack)

	// Add a handler result
	handlerResult := &types.HandlerResult{
		HandlerName: "symlink",
		Files:       []string{"testfile"},
		Status:      "success",
		Message:     "linked to $HOME/.testfile",
		Pack:        "test-pack",
		StartTime:   time.Now(),
		EndTime:     time.Now(),
	}
	packResult.AddHandlerResult(handlerResult)
	packResult.Complete()

	ctx.AddPackResult("test-pack", packResult)
	ctx.Complete()

	// Render
	var buf bytes.Buffer
	renderer := display.NewTextRenderer(&buf)

	err := renderer.RenderExecutionContext(ctx)
	require.NoError(t, err)

	output := buf.String()

	// Check output contains expected elements
	assert.Contains(t, output, "link", "Should contain command name")
	assert.Contains(t, output, "test-pack [status=", "Should contain pack name with status")
	assert.Contains(t, output, "symlink", "Should contain handler name")
	assert.Contains(t, output, "testfile", "Should contain file name")
}

func TestTextRenderer_NilHandling(t *testing.T) {
	var buf bytes.Buffer
	renderer := display.NewTextRenderer(&buf)

	t.Run("nil DisplayResult", func(t *testing.T) {
		buf.Reset()
		err := renderer.Render(nil)
		assert.NoError(t, err)
		assert.Empty(t, buf.String())
	})

	t.Run("nil ExecutionContext", func(t *testing.T) {
		buf.Reset()
		err := renderer.RenderExecutionContext(nil)
		assert.NoError(t, err)
		assert.Empty(t, buf.String())
	})
}
