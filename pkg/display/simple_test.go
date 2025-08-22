package display

import (
	"bytes"
	"strings"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestSimpleRenderer_Render(t *testing.T) {
	tests := []struct {
		name     string
		result   *types.DisplayResult
		expected []string // Lines that should appear in output
	}{
		{
			name: "empty result",
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
			name: "dry run mode",
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
			name: "pack with success files",
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
								Message: "linked to .vimrc",
							},
							{
								Handler: "symlink",
								Path:    "vim/colors/monokai.vim",
								Status:  "success",
								Message: "linked to monokai.vim",
							},
						},
					},
				},
				Timestamp: time.Now(),
			},
			expected: []string{
				"link",
				"vim [status=success]:",
				"symlink",
				"linked to .vimrc [status=success]",
				"linked to monokai.vim [status=success]",
			},
		},
		{
			name: "pack with errors",
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
								Message: "install script failed: exit status 1",
							},
						},
					},
				},
				Timestamp: time.Now(),
			},
			expected: []string{
				"provision",
				"tools [status=alert]:",
				"provision",
				"install script failed: exit status 1 [status=error]",
			},
		},
		{
			name: "mixed status pack",
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "shell",
						Status: "queue",
						Files: []types.DisplayFile{
							{
								Handler: "symlink",
								Path:    "bashrc",
								Status:  "success",
								Message: "linked to .bashrc",
							},
							{
								Handler: "shell_profile",
								Path:    "aliases",
								Status:  "queue",
								Message: "not yet applied",
							},
						},
					},
				},
				Timestamp: time.Now(),
			},
			expected: []string{
				"status",
				"shell [status=queue]:",
				"symlink",
				"shell_profile",
				"not yet applied [status=queue]",
				"linked to .bashrc [status=success]",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var buf bytes.Buffer
			renderer := NewTextRenderer(&buf)

			err := renderer.Render(tt.result)
			testutil.AssertNoError(t, err)

			output := buf.String()
			for _, expected := range tt.expected {
				testutil.AssertTrue(t, strings.Contains(output, expected),
					"Expected output to contain '%s', got:\n%s", expected, output)
			}
		})
	}
}

func TestSimpleRenderer_RenderExecutionContext(t *testing.T) {
	// Create a sample execution context
	ctx := types.NewExecutionContext("link", false)

	// Add a pack result
	pack := &types.Pack{
		Name: "test-pack",
		Path: "/path/to/test-pack",
	}
	packResult := types.NewPackExecutionResult(pack)

	// Add a Handler result
	handlerResult := &types.HandlerResult{
		HandlerName: "symlink",
		Files:       []string{"testfile"},
		Status:      types.StatusReady,
		Message:     "linked to .testfile",
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
	renderer := NewTextRenderer(&buf)

	err := renderer.RenderExecutionContext(ctx)
	testutil.AssertNoError(t, err)

	output := buf.String()

	// Check output contains expected elements
	testutil.AssertTrue(t, strings.Contains(output, "link"), "Should contain command name")
	testutil.AssertTrue(t, strings.Contains(output, "test-pack [status="), "Should contain pack name with status")
	testutil.AssertTrue(t, strings.Contains(output, "symlink"), "Should contain handler name")
	testutil.AssertTrue(t, strings.Contains(output, "linked to $HOME/testfile"), "Should contain Handler-aware message")
	testutil.AssertTrue(t, strings.Contains(output, "[status=success]"), "Should contain status indicator")
}

func TestTruncatePath(t *testing.T) {
	tests := []struct {
		path     string
		maxLen   int
		expected string
	}{
		{"short", 10, "short"},
		{"very/long/path/to/file.txt", 20, "very/lon.../file.txt"},
		{"medium/path/file", 15, "medium...h/file"},
		{"toolong", 5, "toolo"}, // Less than 10 chars, just truncate
	}

	for _, tt := range tests {
		t.Run(tt.path, func(t *testing.T) {
			result := truncatePath(tt.path, tt.maxLen)
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}

func TestSimpleRenderer_ComprehensiveFeatures(t *testing.T) {
	// Test with all display.txxt features
	lastExec := time.Date(2024, 1, 15, 10, 30, 0, 0, time.UTC)

	result := &types.DisplayResult{
		Command: "status",
		Packs: []types.DisplayPack{
			{
				Name:      "neovim",
				Status:    "success",
				HasConfig: true,
				IsIgnored: false,
				Files: []types.DisplayFile{
					{
						Handler:      "config",
						Path:         ".dodot.toml",
						Status:       "config",
						Message:      "dodot config file found",
						IsOverride:   false,
						LastExecuted: nil,
					},
					{
						Handler:      "symlink",
						Path:         ".vimrc",
						Status:       "success",
						Message:      "linked to $HOME/.vimrc",
						IsOverride:   false,
						LastExecuted: &lastExec,
					},
					{
						Handler:      "provision",
						Path:         "setup.sh",
						Status:       "queue",
						Message:      "to be executed",
						IsOverride:   true, // File override example
						LastExecuted: nil,
					},
				},
			},
			{
				Name:      "temp",
				Status:    "ignored",
				HasConfig: false,
				IsIgnored: true,
				Files:     []types.DisplayFile{}, // Ignored packs have no files processed
			},
			{
				Name:   "broken",
				Status: "alert",
				Files: []types.DisplayFile{
					{
						Handler:      "symlink",
						Path:         "config",
						Status:       "error",
						Message:      "failed to symlink $HOME/.config",
						IsOverride:   false,
						LastExecuted: nil,
					},
				},
			},
		},
		Timestamp: time.Now(),
	}

	var buf bytes.Buffer
	renderer := NewTextRenderer(&buf)

	err := renderer.Render(result)
	testutil.AssertNoError(t, err)

	output := buf.String()

	// Verify all features are present
	expectedFeatures := []string{
		"status",                            // Command name
		"neovim [status=success] [config]:", // Pack with config
		"config       : .dodot.toml",        // Config file
		"symlink      : .vimrc",             // Symlink entry
		"linked to $HOME/.vimrc [status=success] [executed=2024-01-15]", // Success with timestamp
		"provision    : *setup.sh",                                      // Override (asterisk) and queue
		"to be executed [status=queue]",                                 // Queue status
		"temp [status=ignored] [ignored]:",                              // Ignored pack
		".dodotignore : dodot is ignoring this dir",                     // Ignored directory message
		"broken [status=alert]:",                                        // Alert pack
		"failed to symlink $HOME/.config [status=error]",                // Error message
	}

	for _, expected := range expectedFeatures {
		testutil.AssertTrue(t, strings.Contains(output, expected),
			"Expected output to contain '%s', got:\n%s", expected, output)
	}

	// Debug output for manual verification
	t.Logf("Comprehensive output:\n%s", output)
}
