package ui_test

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/arthur-debert/dodot/pkg/ui"
	"github.com/arthur-debert/dodot/pkg/ui/display"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewRenderer(t *testing.T) {
	tests := []struct {
		name        string
		format      ui.Format
		expectError bool
		description string
	}{
		{
			name:        "create terminal renderer",
			format:      ui.FormatTerminal,
			expectError: false,
			description: "should create terminal renderer successfully",
		},
		{
			name:        "create text renderer",
			format:      ui.FormatText,
			expectError: false,
			description: "should create text renderer successfully",
		},
		{
			name:        "create json renderer",
			format:      ui.FormatJSON,
			expectError: false,
			description: "should create JSON renderer successfully",
		},
		{
			name:        "create auto renderer with buffer",
			format:      ui.FormatAuto,
			expectError: false,
			description: "should default to terminal format when output is not a file",
		},
		{
			name:        "invalid format",
			format:      ui.Format(999),
			expectError: true,
			description: "should return error for unknown format",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			buf := &bytes.Buffer{}
			renderer, err := ui.NewRenderer(tt.format, buf)

			if tt.expectError {
				assert.Error(t, err)
				assert.Nil(t, renderer)
				assert.Contains(t, err.Error(), "unknown format")
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, renderer)
			}
		})
	}
}

func TestRendererInterface(t *testing.T) {
	// Test that all renderer implementations satisfy the Renderer interface
	formats := []ui.Format{
		ui.FormatTerminal,
		ui.FormatText,
		ui.FormatJSON,
	}

	for _, format := range formats {
		t.Run(format.String()+" renderer implements interface", func(t *testing.T) {
			buf := &bytes.Buffer{}
			renderer, err := ui.NewRenderer(format, buf)
			require.NoError(t, err)

			// Verify the renderer implements all required methods
			assert.NotNil(t, renderer)

			// Test basic method calls (just ensure they don't panic)
			err = renderer.RenderMessage("test message")
			assert.NoError(t, err)

			err = renderer.RenderError(assert.AnError)
			assert.NoError(t, err)

			// Test with simple data
			testData := map[string]string{"test": "data"}
			err = renderer.RenderResult(testData)
			assert.NoError(t, err)
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

	t.Run("render complex result", func(t *testing.T) {
		buf.Reset()
		complexData := &display.DisplayResult{
			Command: "test",
			Packs: []display.DisplayPack{
				{
					Name: "vim",
					Files: []display.DisplayFile{
						{
							Path:    ".vimrc",
							Handler: "symlink",
							Status:  "success",
						},
					},
				},
			},
		}
		err := renderer.RenderResult(complexData)
		assert.NoError(t, err)

		var result display.DisplayResult
		err = json.Unmarshal(buf.Bytes(), &result)
		assert.NoError(t, err)
		assert.Equal(t, "test", result.Command)
		assert.Len(t, result.Packs, 1)
		assert.Equal(t, "vim", result.Packs[0].Name)
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

	t.Run("render unknown result type", func(t *testing.T) {
		buf.Reset()
		unknownData := map[string]string{"foo": "bar"}
		err := renderer.RenderResult(unknownData)
		assert.NoError(t, err)
		output := buf.String()
		assert.Contains(t, output, "map[foo:bar]")
	})
}

func TestTerminalRenderer(t *testing.T) {
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

	t.Run("render unknown result type", func(t *testing.T) {
		buf.Reset()
		unknownData := map[string]string{"foo": "bar"}
		err := renderer.RenderResult(unknownData)
		assert.NoError(t, err)
		output := buf.String()
		assert.Contains(t, output, "map[foo:bar]")
	})
}
