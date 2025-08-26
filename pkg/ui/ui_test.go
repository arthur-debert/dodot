package ui_test

import (
	"bytes"
	"encoding/json"
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewRenderer(t *testing.T) {
	tests := []struct {
		name    string
		format  ui.Format
		wantErr bool
	}{
		{
			name:    "terminal format",
			format:  ui.FormatTerminal,
			wantErr: false,
		},
		{
			name:    "text format",
			format:  ui.FormatText,
			wantErr: false,
		},
		{
			name:    "json format",
			format:  ui.FormatJSON,
			wantErr: false,
		},
		{
			name:    "auto format",
			format:  ui.FormatAuto,
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			buf := &bytes.Buffer{}
			renderer, err := ui.NewRenderer(tt.format, buf)
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, renderer)
			}
		})
	}
}

func TestFormatParsing(t *testing.T) {
	tests := []struct {
		input    string
		expected ui.Format
		wantErr  bool
	}{
		{"auto", ui.FormatAuto, false},
		{"", ui.FormatAuto, false},
		{"term", ui.FormatTerminal, false},
		{"terminal", ui.FormatTerminal, false},
		{"text", ui.FormatText, false},
		{"plain", ui.FormatText, false},
		{"json", ui.FormatJSON, false},
		{"invalid", ui.FormatAuto, true},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			format, err := ui.ParseFormat(tt.input)
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.expected, format)
			}
		})
	}
}

func TestJSONRenderer(t *testing.T) {
	buf := &bytes.Buffer{}
	renderer, err := ui.NewRenderer(ui.FormatJSON, buf)
	require.NoError(t, err)

	t.Run("render message", func(t *testing.T) {
		buf.Reset()
		err := renderer.RenderMessage("hello world")
		assert.NoError(t, err)

		var result map[string]string
		err = json.Unmarshal(buf.Bytes(), &result)
		assert.NoError(t, err)
		assert.Equal(t, "hello world", result["message"])
	})

	t.Run("render error", func(t *testing.T) {
		buf.Reset()
		err := renderer.RenderError(assert.AnError)
		assert.NoError(t, err)

		var result map[string]string
		err = json.Unmarshal(buf.Bytes(), &result)
		assert.NoError(t, err)
		assert.Equal(t, assert.AnError.Error(), result["error"])
	})

	t.Run("render result", func(t *testing.T) {
		buf.Reset()
		testData := map[string]string{"foo": "bar"}
		err := renderer.RenderResult(testData)
		assert.NoError(t, err)

		var result map[string]string
		err = json.Unmarshal(buf.Bytes(), &result)
		assert.NoError(t, err)
		assert.Equal(t, "bar", result["foo"])
	})
}

func TestTextRenderer(t *testing.T) {
	buf := &bytes.Buffer{}
	renderer, err := ui.NewRenderer(ui.FormatText, buf)
	require.NoError(t, err)

	t.Run("render message", func(t *testing.T) {
		buf.Reset()
		err := renderer.RenderMessage("hello world")
		assert.NoError(t, err)
		assert.Equal(t, "hello world\n", buf.String())
	})

	t.Run("render error", func(t *testing.T) {
		buf.Reset()
		err := renderer.RenderError(assert.AnError)
		assert.NoError(t, err)
		assert.Equal(t, "Error: assert.AnError general error for testing\n", buf.String())
	})

	t.Run("render display result", func(t *testing.T) {
		buf.Reset()
		result := &types.DisplayResult{
			Packs: []types.DisplayPack{
				{
					Name: "test",
					Files: []types.DisplayFile{
						{
							Path:    ".vimrc",
							Handler: "symlink",
							Status:  "success",
						},
					},
				},
			},
		}
		err := renderer.RenderResult(result)
		assert.NoError(t, err)
		output := buf.String()
		assert.Contains(t, output, "test [status=success]:")
		assert.Contains(t, output, ".vimrc")
		assert.Contains(t, output, "symlink")
	})
}

func TestTerminalRenderer(t *testing.T) {
	// Only run if we have a real terminal
	if os.Getenv("CI") != "" {
		t.Skip("Skipping terminal tests in CI environment")
	}

	buf := &bytes.Buffer{}
	renderer, err := ui.NewRenderer(ui.FormatTerminal, buf)
	require.NoError(t, err)

	t.Run("render message", func(t *testing.T) {
		buf.Reset()
		err := renderer.RenderMessage("hello world")
		assert.NoError(t, err)
		assert.Contains(t, buf.String(), "hello world")
	})

	t.Run("render error", func(t *testing.T) {
		buf.Reset()
		err := renderer.RenderError(assert.AnError)
		assert.NoError(t, err)
		assert.Contains(t, buf.String(), "assert.AnError")
	})
}

func TestFormatDetection(t *testing.T) {
	// Test detection with NO_COLOR set
	t.Run("NO_COLOR set", func(t *testing.T) {
		oldEnv := os.Getenv("NO_COLOR")
		err := os.Setenv("NO_COLOR", "1")
		require.NoError(t, err)
		defer func() {
			err := os.Setenv("NO_COLOR", oldEnv)
			assert.NoError(t, err)
		}()

		format := ui.DetectFormat(os.Stdout)
		assert.Equal(t, ui.FormatText, format)
	})

	// Test detection with pipe (this test is environment-dependent)
	t.Run("detect from stdout", func(t *testing.T) {
		format := ui.DetectFormat(os.Stdout)
		// Just ensure it returns a valid format
		assert.Contains(t, []ui.Format{ui.FormatTerminal, ui.FormatText}, format)
	})
}
