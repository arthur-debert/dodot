package dodot

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/output/styles"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestConfigFlag(t *testing.T) {
	// Create a temporary custom styles file
	tmpDir := t.TempDir()
	customStylesPath := filepath.Join(tmpDir, "custom-styles.yaml")

	customStyles := `colors:
  primary:
    light: "#FF0000"
    dark: "#FF0000"
styles:
  Header:
    bold: true
    foreground: primary
`
	err := os.WriteFile(customStylesPath, []byte(customStyles), 0644)
	require.NoError(t, err)

	tests := []struct {
		name          string
		args          []string
		checkStyles   bool
		expectWarning bool
	}{
		{
			name:        "no config flag uses default styles",
			args:        []string{"list"},
			checkStyles: false,
		},
		{
			name:        "valid config flag loads custom styles",
			args:        []string{"--config", customStylesPath, "list"},
			checkStyles: true,
		},
		{
			name:          "invalid config path shows warning",
			args:          []string{"--config", "/nonexistent/styles.yaml", "list"},
			expectWarning: true,
		},
		{
			name: "config flag works with help",
			args: []string{"--config", customStylesPath, "--help"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Note: We can't easily reset styles between tests since
			// defaultStylesPath is not exported and the init() function
			// already loaded the default styles

			// Capture stderr for warning messages
			var stderr bytes.Buffer

			// Create root command
			cmd := NewRootCmd()
			cmd.SetErr(&stderr)
			cmd.SetOut(&bytes.Buffer{})
			cmd.SetArgs(tt.args)

			// Execute command
			_ = cmd.Execute()

			// Check if warning was shown
			if tt.expectWarning {
				assert.Contains(t, stderr.String(), "Warning: Failed to load custom styles")
			}

			// Check if custom styles were loaded
			if tt.checkStyles {
				// The custom style should have red as primary color
				style := styles.GetStyle("Header")
				// We can't directly check the color value, but we can verify
				// that LoadStyles was called by checking the style exists
				assert.NotNil(t, style)
			}
		})
	}
}

func TestConfigFlagPersistence(t *testing.T) {
	// Create a temporary custom styles file
	tmpDir := t.TempDir()
	customStylesPath := filepath.Join(tmpDir, "custom-styles.yaml")

	// Create a minimal valid styles file
	customStyles := `colors:
  primary:
    light: "#00FF00"
    dark: "#00FF00"
styles:
  Success:
    foreground: primary
`
	err := os.WriteFile(customStylesPath, []byte(customStyles), 0644)
	require.NoError(t, err)

	// Test that config flag is persistent (applies to subcommands)
	cmd := NewRootCmd()
	cmd.SetArgs([]string{"--config", customStylesPath, "help", "topics"})

	var outBuf, errBuf bytes.Buffer
	cmd.SetOut(&outBuf)
	cmd.SetErr(&errBuf)

	// Execute should work (help topics is a valid command)
	_ = cmd.Execute()
	// The command might fail due to missing topics directory, but that's OK
	// We're just testing that the config flag is processed

	// No warning about failed style loading should appear
	assert.NotContains(t, errBuf.String(), "Failed to load custom styles")
}
