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
				Command:   "deploy",
				Packs:     []types.DisplayPack{},
				Timestamp: time.Now(),
			},
			expected: []string{
				"deploy",
				"No packs to process",
			},
		},
		{
			name: "dry run mode",
			result: &types.DisplayResult{
				Command:   "install",
				DryRun:    true,
				Packs:     []types.DisplayPack{},
				Timestamp: time.Now(),
			},
			expected: []string{
				"install (dry run)",
				"No packs to process",
			},
		},
		{
			name: "pack with success files",
			result: &types.DisplayResult{
				Command: "deploy",
				Packs: []types.DisplayPack{
					{
						Name:   "vim",
						Status: "success",
						Files: []types.DisplayFile{
							{
								PowerUp: "symlink",
								Path:    "vimrc",
								Status:  "success",
								Message: "linked to .vimrc",
							},
							{
								PowerUp: "symlink",
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
				"deploy",
				"vim:",
				"symlink",
				"linked to .vimrc",
				"linked to monokai.vim",
			},
		},
		{
			name: "pack with errors",
			result: &types.DisplayResult{
				Command: "install",
				Packs: []types.DisplayPack{
					{
						Name:   "tools",
						Status: "alert",
						Files: []types.DisplayFile{
							{
								PowerUp: "install_script",
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
				"install",
				"tools:",
				"install_script",
				"install script failed",
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
								PowerUp: "symlink",
								Path:    "bashrc",
								Status:  "success",
								Message: "linked to .bashrc",
							},
							{
								PowerUp: "shell_profile",
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
				"shell:",
				"symlink",
				"shell_profile",
				"not yet applied",
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
	ctx := types.NewExecutionContext("deploy", false)

	// Add a pack result
	pack := &types.Pack{
		Name: "test-pack",
		Path: "/path/to/test-pack",
	}
	packResult := types.NewPackExecutionResult(pack)

	// Add a PowerUp result
	powerUpResult := &types.PowerUpResult{
		PowerUpName: "symlink",
		Files:       []string{"testfile"},
		Status:      types.StatusReady,
		Message:     "linked to .testfile",
		Pack:        "test-pack",
		StartTime:   time.Now(),
		EndTime:     time.Now(),
	}
	packResult.AddPowerUpResult(powerUpResult)
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
	testutil.AssertTrue(t, strings.Contains(output, "deploy"), "Should contain command name")
	testutil.AssertTrue(t, strings.Contains(output, "test-pack:"), "Should contain pack name with colon")
	testutil.AssertTrue(t, strings.Contains(output, "symlink"), "Should contain powerup name")
	testutil.AssertTrue(t, strings.Contains(output, "linked to $HOME/testfile"), "Should contain PowerUp-aware message")
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
