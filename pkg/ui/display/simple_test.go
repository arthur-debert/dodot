package display_test

import (
	"bytes"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/execution/context"
	"github.com/arthur-debert/dodot/pkg/execution/results"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/display"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestTextRenderer_Render(t *testing.T) {
	tests := []struct {
		name        string
		result      *display.DisplayResult
		expected    []string // Lines that should appear in output
		notExpected []string // Lines that should NOT appear
	}{
		{
			name: "empty result with no packs",
			result: &display.DisplayResult{
				Command:   "link",
				Packs:     []display.DisplayPack{},
				Timestamp: time.Now(),
			},
			expected: []string{
				"link",
				"No packs to process",
			},
		},
		{
			name: "dry run mode indicator",
			result: &display.DisplayResult{
				Command:   "provision",
				DryRun:    true,
				Packs:     []display.DisplayPack{},
				Timestamp: time.Now(),
			},
			expected: []string{
				"provision (dry run)",
				"No packs to process",
			},
		},
		{
			name: "pack with successful files",
			result: &display.DisplayResult{
				Command: "link",
				Packs: []display.DisplayPack{
					{
						Name:   "vim",
						Status: "success",
						Files: []display.DisplayFile{
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
			result: &display.DisplayResult{
				Command: "provision",
				Packs: []display.DisplayPack{
					{
						Name:   "tools",
						Status: "alert",
						Files: []display.DisplayFile{
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
			result: &display.DisplayResult{
				Command: "status",
				Packs: []display.DisplayPack{
					{
						Name:   "shell",
						Status: "alert",
						Files: []display.DisplayFile{
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
			result: &display.DisplayResult{
				Command: "status",
				Packs: []display.DisplayPack{
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
			result: &display.DisplayResult{
				Command: "status",
				Packs: []display.DisplayPack{
					{
						Name:      "neovim",
						Status:    "success",
						HasConfig: true,
						Files: []display.DisplayFile{
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
			result: &display.DisplayResult{
				Command: "link",
				Packs: []display.DisplayPack{
					{
						Name:   "vim",
						Status: "success",
						Files: []display.DisplayFile{
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
			result: &display.DisplayResult{
				Command: "status",
				Packs: []display.DisplayPack{
					{
						Name:   "vim",
						Status: "success",
						Files: []display.DisplayFile{
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
			result: &display.DisplayResult{
				Command: "status",
				Packs: []display.DisplayPack{
					{
						Name:   "empty-pack",
						Status: "success",
						Files:  []display.DisplayFile{},
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
			result: &display.DisplayResult{
				Command: "status",
				Packs: []display.DisplayPack{
					{
						Name:   "zsh",
						Status: "success",
						Files: []display.DisplayFile{
							{Handler: "symlink", Path: ".zshrc", Status: "success", Message: "linked"},
						},
					},
					{
						Name:   "bash",
						Status: "success",
						Files: []display.DisplayFile{
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

func TestTextRenderer_RenderWithComplexData(t *testing.T) {
	// Create a sample execution context
	ctxManager := context.NewManager()
	ctx := ctxManager.CreateContext("link", false)

	// Add a pack result
	pack := &types.Pack{
		Name: "test-pack",
		Path: "/path/to/test-pack",
	}
	aggregator := results.NewAggregator()
	packResult := aggregator.CreatePackResult(pack)

	// Add a handler result
	handlerResult := &context.HandlerResult{
		HandlerName: "symlink",
		Files:       []string{"testfile"},
		Status:      "success",
		Message:     "linked to $HOME/.testfile",
		Pack:        "test-pack",
		StartTime:   time.Now(),
		EndTime:     time.Now(),
	}
	aggregator.AddHandlerResult(packResult, handlerResult)
	aggregator.CompletePackResult(packResult)

	ctxManager.AddPackResult(ctx, "test-pack", packResult)
	ctxManager.CompleteContext(ctx)

	// Render
	var buf bytes.Buffer
	renderer := display.NewTextRenderer(&buf)

	// Convert to display result first
	displayResult := &display.DisplayResult{
		Command: ctx.Command,
		Packs: []display.DisplayPack{{
			Name:   "test-pack",
			Status: "success",
			Files: []display.DisplayFile{{
				Handler: "symlink",
				Path:    "testfile",
				Status:  "success",
				Message: "linked to $HOME/testfile",
			}},
		}},
		DryRun:    ctx.DryRun,
		Timestamp: ctx.EndTime,
	}
	err := renderer.Render(displayResult)
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

	t.Run("nil DisplayResult again", func(t *testing.T) {
		buf.Reset()
		err := renderer.Render(nil)
		assert.NoError(t, err)
		assert.Empty(t, buf.String())
	})
}
