// pkg/ui/output/renderer_test.go  
// TEST TYPE: Output Rendering Test
// DEPENDENCIES: None (pure data transformation)
// PURPOSE: Test rendering of data structures to terminal output

package output_test

import (
	"bytes"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/output"
)

func TestRenderer_Render(t *testing.T) {
	tests := []struct {
		name        string
		result      *types.DisplayResult
		noColor     bool
		wantStrings []string
		skipStrings []string
	}{
		{
			name: "successful_status_command",
			result: &types.DisplayResult{
				Command: "status",
				DryRun:  false,
				Packs: []types.DisplayPack{
					{
						Name:   "vim",
						Status: "success",
						Files: []types.DisplayFile{
							{
								Handler: "symlink",
								Path:    ".vimrc",
								Status:  "success",
								Message: "linked",
							},
						},
					},
				},
			},
			noColor: true,
			wantStrings: []string{
				"vim",
				".vimrc",
				"linked",
			},
			skipStrings: []string{
				"ERROR",
				"FAILED",
				"\x1b[", // no color codes
			},
		},
		{
			name: "dry_run_shows_preview",
			result: &types.DisplayResult{
				Command: "link",
				DryRun:  true,
				Packs: []types.DisplayPack{
					{
						Name:   "git",
						Status: "queue",
						Files: []types.DisplayFile{
							{
								Handler: "symlink",
								Path:    ".gitconfig",
								Status:  "queue",
								Message: "will be linked",
							},
						},
					},
				},
			},
			noColor: false,
			wantStrings: []string{
				"DRY RUN",
				"git",
				".gitconfig",
				"will be linked",
			},
			skipStrings: []string{
				"ERROR",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			buf := &bytes.Buffer{}
			renderer, err := output.NewRenderer(buf, tt.noColor)
			if err != nil {
				t.Fatalf("NewRenderer failed: %v", err)
			}

			err = renderer.Render(tt.result)
			if err != nil {
				t.Fatalf("Render failed: %v", err)
			}

			result := buf.String()

			// Check expected strings are present
			for _, want := range tt.wantStrings {
				if !strings.Contains(result, want) {
					t.Errorf("output missing expected string %q\nGot:\n%s", want, result)
				}
			}

			// Check unwanted strings are absent
			for _, skip := range tt.skipStrings {
				if strings.Contains(result, skip) {
					t.Errorf("output contains unwanted string %q\nGot:\n%s", skip, result)
				}
			}
		})
	}
}

func TestRenderer_RenderExecutionContext(t *testing.T) {
	tests := []struct {
		name        string
		context     *types.ExecutionContext
		noColor     bool
		wantStrings []string
	}{
		{
			name: "basic_execution_context",
			context: &types.ExecutionContext{
				Command: "link",
				DryRun:  false,
				PackResults: []types.PackResult{
					{
						Pack:   types.Pack{Name: "vim"},
						Status: types.ExecutionStatusSuccess,
						HandlerResults: []*types.HandlerResult{
							{
								HandlerName: "symlink",
								Status:      types.StatusReady,
							},
						},
					},
				},
			},
			noColor: true,
			wantStrings: []string{
				"vim",
				"symlink",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			buf := &bytes.Buffer{}
			renderer, err := output.NewRenderer(buf, tt.noColor)
			if err != nil {
				t.Fatalf("NewRenderer failed: %v", err)
			}

			err = renderer.RenderExecutionContext(tt.context)
			if err != nil {
				t.Fatalf("RenderExecutionContext failed: %v", err)
			}

			result := buf.String()

			for _, want := range tt.wantStrings {
				if !strings.Contains(result, want) {
					t.Errorf("output missing expected string %q\nGot:\n%s", want, result)
				}
			}
		})
	}
}

func TestRenderer_ColorHandling(t *testing.T) {
	result := &types.DisplayResult{
		Command: "status",
		Packs: []types.DisplayPack{
			{
				Name:   "vim",
				Status: "success",
			},
		},
	}

	t.Run("with_color", func(t *testing.T) {
		buf := &bytes.Buffer{}
		renderer, err := output.NewRenderer(buf, false)
		if err != nil {
			t.Fatalf("NewRenderer failed: %v", err)
		}

		err = renderer.Render(result)
		if err != nil {
			t.Fatalf("Render failed: %v", err)
		}

		// Should contain ANSI color codes
		if !strings.Contains(buf.String(), "\x1b[") {
			t.Error("expected ANSI color codes in output")
		}
	})

	t.Run("without_color", func(t *testing.T) {
		buf := &bytes.Buffer{}
		renderer, err := output.NewRenderer(buf, true)
		if err != nil {
			t.Fatalf("NewRenderer failed: %v", err)
		}

		err = renderer.Render(result)
		if err != nil {
			t.Fatalf("Render failed: %v", err)
		}

		// Should not contain ANSI color codes
		if strings.Contains(buf.String(), "\x1b[") {
			t.Error("unexpected ANSI color codes in no-color output")
		}
	})
}