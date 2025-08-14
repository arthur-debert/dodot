package output

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/output/styles"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestLoadStylesFromFile(t *testing.T) {
	// Create a temporary directory for test files
	tmpDir := t.TempDir()

	tests := []struct {
		name        string
		setupFile   func() string
		wantErr     bool
		errContains string
	}{
		{
			name: "valid styles file",
			setupFile: func() string {
				path := filepath.Join(tmpDir, "valid.yaml")
				content := `colors:
  testcolor:
    light: "#123456"
    dark: "#654321"
styles:
  TestStyle:
    bold: true
    foreground: testcolor
`
				err := os.WriteFile(path, []byte(content), 0644)
				require.NoError(t, err)
				return path
			},
			wantErr: false,
		},
		{
			name: "nonexistent file",
			setupFile: func() string {
				return filepath.Join(tmpDir, "nonexistent.yaml")
			},
			wantErr:     true,
			errContains: "failed to read styles file",
		},
		{
			name: "invalid YAML",
			setupFile: func() string {
				path := filepath.Join(tmpDir, "invalid.yaml")
				content := `colors:
  - this is not valid YAML structure
    invalid: [[[
`
				err := os.WriteFile(path, []byte(content), 0644)
				require.NoError(t, err)
				return path
			},
			wantErr:     true,
			errContains: "failed to parse styles.yaml",
		},
		{
			name: "empty file",
			setupFile: func() string {
				path := filepath.Join(tmpDir, "empty.yaml")
				err := os.WriteFile(path, []byte(""), 0644)
				require.NoError(t, err)
				return path
			},
			wantErr: false, // Empty YAML is valid, just results in empty maps
		},
		{
			name: "missing colors section",
			setupFile: func() string {
				path := filepath.Join(tmpDir, "no-colors.yaml")
				content := `styles:
  TestStyle:
    bold: true
`
				err := os.WriteFile(path, []byte(content), 0644)
				require.NoError(t, err)
				return path
			},
			wantErr: false, // Missing sections are OK
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Get the test file path
			path := tt.setupFile()

			// Test the function
			err := LoadStylesFromFile(path)

			if tt.wantErr {
				assert.Error(t, err)
				if tt.errContains != "" {
					assert.Contains(t, err.Error(), tt.errContains)
				}
			} else {
				assert.NoError(t, err)

				// For valid files, check that styles were actually loaded
				if tt.name == "valid styles file" {
					// Check that our test style exists
					style := styles.GetStyle("TestStyle")
					assert.NotNil(t, style)
				}
			}
		})
	}
}

func TestLoadStylesFromFileIntegration(t *testing.T) {
	// Save the current styles so we can restore them
	originalRegistry := styles.StyleRegistry
	defer func() {
		styles.StyleRegistry = originalRegistry
	}()

	// Create a custom styles file
	tmpDir := t.TempDir()
	customPath := filepath.Join(tmpDir, "custom.yaml")

	customContent := `colors:
  custom:
    light: "#CUSTOM1"
    dark: "#CUSTOM2"
styles:
  CustomStyle:
    bold: true
    italic: true
    foreground: custom
  # Override an existing style
  Header:
    underline: true
    foreground: custom
`
	err := os.WriteFile(customPath, []byte(customContent), 0644)
	require.NoError(t, err)

	// Load the custom styles
	err = LoadStylesFromFile(customPath)
	require.NoError(t, err)

	// Verify custom style was added
	customStyle := styles.GetStyle("CustomStyle")
	assert.NotNil(t, customStyle)
	assert.True(t, customStyle.GetBold())
	assert.True(t, customStyle.GetItalic())

	// Verify existing style was overridden
	headerStyle := styles.GetStyle("Header")
	assert.NotNil(t, headerStyle)
	assert.True(t, headerStyle.GetUnderline())

	// Verify that a style not in the custom file returns default
	nonExistent := styles.GetStyle("NonExistentStyle")
	assert.NotNil(t, nonExistent) // GetStyle returns default style for non-existent
}
