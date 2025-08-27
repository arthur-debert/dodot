package styles

import (
	"path/filepath"
	"runtime"
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/stretchr/testify/assert"
)

func init() {
	// For tests, ensure we load from the correct path relative to the test file
	_, filename, _, _ := runtime.Caller(0)
	dir := filepath.Dir(filename)
	stylesPath := filepath.Join(dir, "styles.yaml")
	if err := LoadStyles(stylesPath); err != nil {
		panic("failed to load styles for tests: " + err.Error())
	}
}

func TestStyleRegistry(t *testing.T) {
	// Test that all expected styles are present
	expectedStyles := []string{
		"Header", "SubHeader", "PackHeader", "CommandHeader",
		"Success", "Error", "Warning", "Info", "Queued", "Ignored",
		"SuccessBadge", "ErrorBadge", "WarningBadge",
		"Bold", "Italic", "Underline", "Muted", "MutedItalic",
		"Handler", "FilePath", "ConfigFile", "Override",
		"Indent", "DoubleIndent", "Section",
		"Timestamp", "DryRunBanner", "NoContent",
		"TableHeader", "TableCell", "TableSeparator",
	}

	for _, styleName := range expectedStyles {
		t.Run(styleName, func(t *testing.T) {
			style, exists := StyleRegistry[styleName]
			assert.True(t, exists, "Style %s should exist in registry", styleName)
			assert.NotNil(t, style, "Style %s should not be nil", styleName)
		})
	}
}

func TestGetStyle(t *testing.T) {
	tests := []struct {
		name      string
		styleName string
		exists    bool
	}{
		{
			name:      "returns existing style",
			styleName: "Success",
			exists:    true,
		},
		{
			name:      "returns default style for non-existent",
			styleName: "NonExistentStyle",
			exists:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			style := GetStyle(tt.styleName)
			assert.NotNil(t, style)

			if tt.exists {
				// Should return the actual style from registry
				registryStyle := StyleRegistry[tt.styleName]
				assert.Equal(t, registryStyle, style)
			} else {
				// Should return a default empty style
				assert.Equal(t, lipgloss.NewStyle(), style)
			}
		})
	}
}

func TestMergeStyles(t *testing.T) {
	tests := []struct {
		name   string
		styles []string
	}{
		{
			name:   "merges single style",
			styles: []string{"Bold"},
		},
		{
			name:   "merges multiple styles",
			styles: []string{"Bold", "Underline"},
		},
		{
			name:   "handles non-existent styles",
			styles: []string{"Bold", "NonExistent", "Italic"},
		},
		{
			name:   "handles empty list",
			styles: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			merged := MergeStyles(tt.styles...)
			assert.NotNil(t, merged)
			// The merged style should not panic when rendered
			_ = merged.Render("test")
		})
	}
}

func TestYAMLConfiguration(t *testing.T) {
	// Test that YAML was loaded successfully
	assert.NotNil(t, colors, "Colors map should be initialized")
	assert.NotEmpty(t, colors, "Colors map should contain entries")
	assert.NotNil(t, StyleRegistry, "StyleRegistry should be initialized")
	assert.NotEmpty(t, StyleRegistry, "StyleRegistry should contain entries")
}

func TestAdaptiveColors(t *testing.T) {
	// Test that all adaptive colors are properly defined
	expectedColors := []string{
		"primary", "secondary", "muted",
		"success", "error", "warning", "info", "queued", "ignored",
		"successBg", "errorBg", "warningBg",
	}

	for _, colorName := range expectedColors {
		t.Run(colorName, func(t *testing.T) {
			color, exists := colors[colorName]
			assert.True(t, exists, "Color %s should exist", colorName)
			assert.NotEmpty(t, color.Light, "%s should have Light color defined", colorName)
			assert.NotEmpty(t, color.Dark, "%s should have Dark color defined", colorName)
		})
	}
}

func TestStyleProperties(t *testing.T) {
	// Test specific style properties
	tests := []struct {
		name        string
		styleName   string
		checkBold   bool
		wantBold    bool
		checkItalic bool
		wantItalic  bool
	}{
		{
			name:      "Header should be bold",
			styleName: "Header",
			checkBold: true,
			wantBold:  true,
		},
		{
			name:        "Ignored should be italic",
			styleName:   "Ignored",
			checkItalic: true,
			wantItalic:  true,
		},
		{
			name:      "Bold style should be bold",
			styleName: "Bold",
			checkBold: true,
			wantBold:  true,
		},
		{
			name:        "Italic style should be italic",
			styleName:   "Italic",
			checkItalic: true,
			wantItalic:  true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			style := GetStyle(tt.styleName)

			if tt.checkBold {
				bold := style.GetBold()
				assert.Equal(t, tt.wantBold, bold, "Bold property mismatch")
			}

			if tt.checkItalic {
				italic := style.GetItalic()
				assert.Equal(t, tt.wantItalic, italic, "Italic property mismatch")
			}
		})
	}
}
