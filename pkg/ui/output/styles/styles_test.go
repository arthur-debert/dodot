package styles_test

import (
	"path/filepath"
	"runtime"
	"testing"

	"github.com/arthur-debert/dodot/pkg/ui/output/styles"
	"github.com/charmbracelet/lipgloss"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func init() {
	// For tests, ensure we load from the correct path relative to the test file
	_, filename, _, ok := runtime.Caller(0)
	if !ok {
		panic("failed to get runtime caller info")
	}

	dir := filepath.Dir(filename)
	stylesPath := filepath.Join(dir, "styles.yaml")
	if err := styles.LoadStyles(stylesPath); err != nil {
		panic("failed to load styles for tests: " + err.Error())
	}
}

func TestStyleRegistry(t *testing.T) {
	// Test that all expected styles are present
	expectedStyles := []string{
		// Headers
		"Header", "SubHeader", "PackHeader", "CommandHeader",
		// Status styles
		"Success", "Error", "Warning", "Info", "Queued", "Ignored",
		// Badge styles
		"SuccessBadge", "ErrorBadge", "WarningBadge",
		// Text formatting
		"Bold", "Italic", "Underline", "Muted", "MutedItalic",
		// Content types
		"Handler", "FilePath", "ConfigFile", "Override",
		// Layout
		"Indent", "DoubleIndent", "Section",
		// Special
		"Timestamp", "DryRunBanner", "NoContent",
		// Table styles
		"TableHeader", "TableCell", "TableSeparator",
	}

	for _, styleName := range expectedStyles {
		t.Run(styleName, func(t *testing.T) {
			style, exists := styles.StyleRegistry[styleName]
			assert.True(t, exists, "Style %s should exist in registry", styleName)
			assert.NotNil(t, style, "Style %s should not be nil", styleName)
		})
	}

	// Ensure we have the expected number of styles (helps catch removals)
	assert.GreaterOrEqual(t, len(styles.StyleRegistry), len(expectedStyles),
		"StyleRegistry should contain at least %d styles", len(expectedStyles))
}

func TestGetStyle(t *testing.T) {
	tests := []struct {
		name        string
		styleName   string
		shouldExist bool
		description string
	}{
		{
			name:        "existing style Success",
			styleName:   "Success",
			shouldExist: true,
			description: "should return the Success style from registry",
		},
		{
			name:        "existing style Error",
			styleName:   "Error",
			shouldExist: true,
			description: "should return the Error style from registry",
		},
		{
			name:        "non-existent style",
			styleName:   "NonExistentStyle",
			shouldExist: false,
			description: "should return default empty style for non-existent style",
		},
		{
			name:        "empty string style name",
			styleName:   "",
			shouldExist: false,
			description: "should return default empty style for empty name",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			style := styles.GetStyle(tt.styleName)
			assert.NotNil(t, style, "GetStyle should never return nil")

			if tt.shouldExist {
				// Should return the actual style from registry
				registryStyle, exists := styles.StyleRegistry[tt.styleName]
				assert.True(t, exists, "Style should exist in registry")
				assert.Equal(t, registryStyle, style, "Should return registry style")
			} else {
				// Should return a default empty style
				assert.Equal(t, lipgloss.NewStyle(), style, "Should return default style")
			}

			// Ensure the style can be used without panic
			rendered := style.Render("test content")
			assert.NotEmpty(t, rendered, "Style should render content")
		})
	}
}

func TestMergeStyles(t *testing.T) {
	tests := []struct {
		name        string
		styles      []string
		description string
	}{
		{
			name:        "single style",
			styles:      []string{"Bold"},
			description: "should merge a single style",
		},
		{
			name:        "multiple compatible styles",
			styles:      []string{"Bold", "Underline"},
			description: "should merge multiple styles without conflict",
		},
		{
			name:        "styles with color and formatting",
			styles:      []string{"Success", "Bold"},
			description: "should merge color and formatting styles",
		},
		{
			name:        "with non-existent style",
			styles:      []string{"Bold", "NonExistent", "Italic"},
			description: "should gracefully handle non-existent styles",
		},
		{
			name:        "empty list",
			styles:      []string{},
			description: "should return empty style for empty list",
		},
		{
			name:        "duplicate styles",
			styles:      []string{"Bold", "Bold", "Italic"},
			description: "should handle duplicate styles",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			merged := styles.MergeStyles(tt.styles...)
			assert.NotNil(t, merged, "MergeStyles should never return nil")

			// The merged style should render without panic
			result := merged.Render("test content")
			assert.NotEmpty(t, result, "Merged style should render content")
		})
	}
}

func TestYAMLConfiguration(t *testing.T) {
	// Test that YAML was loaded successfully
	assert.NotNil(t, styles.StyleRegistry, "StyleRegistry should be initialized")
	assert.NotEmpty(t, styles.StyleRegistry, "StyleRegistry should contain entries")

	// Verify that styles have been properly loaded with properties
	t.Run("verify style properties loaded", func(t *testing.T) {
		// Check a few known styles exist
		successStyle := styles.GetStyle("Success")
		errorStyle := styles.GetStyle("Error")
		boldStyle := styles.GetStyle("Bold")

		assert.NotNil(t, successStyle, "Success style should exist")
		assert.NotNil(t, errorStyle, "Error style should exist")
		assert.NotNil(t, boldStyle, "Bold style should exist")

		// Verify they're not the default empty style
		assert.NotEqual(t, lipgloss.NewStyle(), successStyle,
			"Success should not be default style")
		assert.NotEqual(t, lipgloss.NewStyle(), errorStyle,
			"Error should not be default style")
	})
}

func TestStyleProperties(t *testing.T) {
	// Test specific style properties based on styles.yaml
	tests := []struct {
		name           string
		styleName      string
		checkBold      bool
		expectedBold   bool
		checkItalic    bool
		expectedItalic bool
		description    string
	}{
		{
			name:         "Header style",
			styleName:    "Header",
			checkBold:    true,
			expectedBold: true,
			description:  "Header should be bold for emphasis",
		},
		{
			name:           "Ignored style",
			styleName:      "Ignored",
			checkItalic:    true,
			expectedItalic: true,
			description:    "Ignored items should be italic",
		},
		{
			name:         "Bold style",
			styleName:    "Bold",
			checkBold:    true,
			expectedBold: true,
			description:  "Bold style should apply bold formatting",
		},
		{
			name:           "Italic style",
			styleName:      "Italic",
			checkItalic:    true,
			expectedItalic: true,
			description:    "Italic style should apply italic formatting",
		},
		{
			name:           "MutedItalic style",
			styleName:      "MutedItalic",
			checkItalic:    true,
			expectedItalic: true,
			description:    "MutedItalic should have italic formatting",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			style := styles.GetStyle(tt.styleName)
			require.NotNil(t, style, "Style should exist")

			if tt.checkBold {
				bold := style.GetBold()
				assert.Equal(t, tt.expectedBold, bold,
					"Bold property mismatch for %s: %s", tt.styleName, tt.description)
			}

			if tt.checkItalic {
				italic := style.GetItalic()
				assert.Equal(t, tt.expectedItalic, italic,
					"Italic property mismatch for %s: %s", tt.styleName, tt.description)
			}
		})
	}
}

func TestLoadStyles(t *testing.T) {
	t.Run("load from valid path", func(t *testing.T) {
		// Get the path to styles.yaml
		_, filename, _, ok := runtime.Caller(0)
		require.True(t, ok, "Should get runtime caller info")

		dir := filepath.Dir(filename)
		stylesPath := filepath.Join(dir, "styles.yaml")

		// Load styles (this should succeed)
		err := styles.LoadStyles(stylesPath)
		assert.NoError(t, err, "Should load styles from valid path")
		assert.NotEmpty(t, styles.StyleRegistry, "Should populate style registry")
	})

	t.Run("error on non-existent file", func(t *testing.T) {
		err := styles.LoadStyles("/non/existent/path/styles.yaml")
		assert.Error(t, err, "Should error on non-existent file")
		assert.Contains(t, err.Error(), "failed to read styles file")
	})
}

func TestStyleRendering(t *testing.T) {
	// Test that various styles render content correctly
	testContent := "Test Content"

	styleNames := []string{
		"Header", "Success", "Error", "Warning",
		"Bold", "Italic", "Underline",
		"Handler", "FilePath", "ConfigFile",
	}

	for _, styleName := range styleNames {
		t.Run(styleName, func(t *testing.T) {
			style := styles.GetStyle(styleName)
			rendered := style.Render(testContent)

			// At minimum, the content should be present
			assert.Contains(t, rendered, testContent,
				"Rendered output should contain the original content")
		})
	}
}

func TestEdgeCases(t *testing.T) {
	t.Run("GetStyle with special characters", func(t *testing.T) {
		specialNames := []string{
			"Style With Spaces",
			"Style-With-Dashes",
			"Style.With.Dots",
			"Style/With/Slashes",
			"🎨StyleWithEmoji",
		}

		for _, name := range specialNames {
			style := styles.GetStyle(name)
			assert.NotNil(t, style, "Should return non-nil style for: %s", name)
			assert.Equal(t, lipgloss.NewStyle(), style,
				"Should return default style for non-existent: %s", name)
		}
	})

	t.Run("MergeStyles with nil-like values", func(t *testing.T) {
		// Should handle empty strings in the list
		merged := styles.MergeStyles("Bold", "", "Italic")
		assert.NotNil(t, merged)

		// Should be able to render
		result := merged.Render("test")
		assert.NotEmpty(t, result)
	})
}
