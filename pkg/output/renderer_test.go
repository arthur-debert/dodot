package output

import (
	"bytes"
	"strings"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestRenderer_Render(t *testing.T) {
	tests := []struct {
		name        string
		noColor     bool
		result      *types.DisplayResult
		wantContent []string
		notWant     []string
	}{
		{
			name:    "renders basic result with colors",
			noColor: false,
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "git",
						Status: "success",
						Files: []types.DisplayFile{
							{
								PowerUp: "symlink",
								Path:    ".gitconfig",
								Status:  "success",
								Message: "linked",
							},
						},
					},
				},
			},
			wantContent: []string{
				"Command: status",
				"git",
				".gitconfig",
				"deployed",
			},
			notWant: []string{
				"<", ">", // No XML tags in output
			},
		},
		{
			name:    "renders result without colors",
			noColor: true,
			result: &types.DisplayResult{
				Command: "deploy",
				DryRun:  true,
				Packs: []types.DisplayPack{
					{
						Name:   "vim",
						Status: "queue",
						Files: []types.DisplayFile{
							{
								PowerUp: "symlink",
								Path:    ".vimrc",
								Status:  "queue",
								Message: "will be linked",
							},
						},
					},
				},
			},
			wantContent: []string{
				"DRY RUN MODE",
				"Command: deploy",
				"vim",
				".vimrc",
				"queued",
			},
			notWant: []string{
				"<", ">", // No XML tags in output
				"\x1b", // No ANSI codes
			},
		},
		{
			name:    "renders error status",
			noColor: false,
			result: &types.DisplayResult{
				Command: "install",
				Packs: []types.DisplayPack{
					{
						Name:   "broken",
						Status: "error",
						Files: []types.DisplayFile{
							{
								PowerUp: "symlink",
								Path:    ".broken",
								Status:  "error",
								Message: "permission denied",
							},
						},
					},
				},
			},
			wantContent: []string{
				"broken",
				".broken",
				"error",
			},
		},
		{
			name:    "renders config and ignore files",
			noColor: true,
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "test",
						Status: "success",
						Files: []types.DisplayFile{
							{
								PowerUp: "",
								Path:    ".dodot.toml",
								Status:  "config",
								Message: "",
							},
							{
								PowerUp: "",
								Path:    "ignored_dir",
								Status:  "ignored",
								Message: "",
							},
						},
					},
				},
			},
			wantContent: []string{
				"config",
				".dodot.toml",
				"ignore",
				"ignored_dir",
				"ignored",
			},
		},
		{
			name:    "renders override indicator",
			noColor: true,
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "custom",
						Status: "success",
						Files: []types.DisplayFile{
							{
								PowerUp:    "profile",
								Path:       ".bashrc",
								Status:     "success",
								Message:    "custom profile",
								IsOverride: true,
							},
						},
					},
				},
			},
			wantContent: []string{
				".bashrc",
				"[override]",
				"deployed",
			},
		},
		{
			name:    "renders timestamp",
			noColor: true,
			result: &types.DisplayResult{
				Command:   "deploy",
				Timestamp: time.Date(2024, 1, 15, 10, 30, 0, 0, time.UTC),
				Packs: []types.DisplayPack{
					{
						Name:   "test",
						Status: "success",
					},
				},
			},
			wantContent: []string{
				"Executed at: 2024-01-15 10:30:00",
			},
		},
		{
			name:    "renders empty packs message",
			noColor: true,
			result: &types.DisplayResult{
				Command: "status",
				Packs:   []types.DisplayPack{},
			},
			wantContent: []string{
				"No packs found.",
			},
		},
		{
			name:    "renders pack with no files",
			noColor: true,
			result: &types.DisplayResult{
				Command: "status",
				Packs: []types.DisplayPack{
					{
						Name:   "empty",
						Status: "success",
						Files:  []types.DisplayFile{},
					},
				},
			},
			wantContent: []string{
				"empty",
				"No files to process.",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var buf bytes.Buffer
			renderer, err := NewRenderer(&buf, tt.noColor)
			require.NoError(t, err)

			err = renderer.Render(tt.result)
			require.NoError(t, err)

			output := buf.String()

			// Check wanted content
			for _, want := range tt.wantContent {
				assert.Contains(t, output, want, "Output should contain: %s", want)
			}

			// Check unwanted content
			for _, notWant := range tt.notWant {
				assert.NotContains(t, output, notWant, "Output should not contain: %s", notWant)
			}
		})
	}
}

func TestRenderer_RenderExecutionContext(t *testing.T) {
	var buf bytes.Buffer
	renderer, err := NewRenderer(&buf, true) // noColor for predictable output
	require.NoError(t, err)

	// Create a test pack
	testPack := &types.Pack{
		Name: "test-pack",
		Path: "/test/path",
	}

	ctx := &types.ExecutionContext{
		Command: "test",
		PackResults: map[string]*types.PackExecutionResult{
			"test-pack": {
				Pack: testPack,
				PowerUpResults: []*types.PowerUpResult{
					{
						PowerUpName: "symlink",
						Files:       []string{".testfile"},
						Status:      types.StatusReady,
						Message:     "test success",
						Pack:        "test-pack",
					},
				},
				Status: types.ExecutionStatusSuccess,
			},
		},
		DryRun: false,
	}

	// Convert to DisplayResult to test the rendering
	displayResult := ctx.ToDisplayResult()
	err = renderer.Render(displayResult)
	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "Command: test")
	assert.Contains(t, output, "test-pack")
	assert.Contains(t, output, ".testfile")
	assert.Contains(t, output, "deployed")
}

func TestRenderer_RenderError(t *testing.T) {
	tests := []struct {
		name    string
		noColor bool
		err     error
		want    string
		notWant string
	}{
		{
			name:    "renders error with color",
			noColor: false,
			err:     assert.AnError,
			want:    "Error:",
			notWant: "<Error>",
		},
		{
			name:    "renders error without color",
			noColor: true,
			err:     assert.AnError,
			want:    "Error: assert.AnError general error for testing",
			notWant: "\x1b",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var buf bytes.Buffer
			renderer, err := NewRenderer(&buf, tt.noColor)
			require.NoError(t, err)

			err = renderer.RenderError(tt.err)
			require.NoError(t, err)

			output := buf.String()
			assert.Contains(t, output, tt.want)
			if tt.notWant != "" {
				assert.NotContains(t, output, tt.notWant)
			}
		})
	}
}

func TestRenderer_RenderMessage(t *testing.T) {
	tests := []struct {
		name    string
		noColor bool
		style   string
		message string
		want    string
	}{
		{
			name:    "renders styled message with color",
			noColor: false,
			style:   "Success",
			message: "Operation completed",
			want:    "Operation completed",
		},
		{
			name:    "renders message without color",
			noColor: true,
			style:   "Error",
			message: "Something went wrong",
			want:    "Something went wrong",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var buf bytes.Buffer
			renderer, err := NewRenderer(&buf, tt.noColor)
			require.NoError(t, err)

			err = renderer.RenderMessage(tt.style, tt.message)
			require.NoError(t, err)

			output := strings.TrimSpace(buf.String())
			assert.Contains(t, output, tt.want)
			if tt.noColor {
				assert.NotContains(t, output, "<")
				assert.NotContains(t, output, ">")
			}
		})
	}
}
