// Package styles defines the visual styling for dodot's terminal output.
//
// All styles use semantic names and adaptive colors that automatically
// adjust to light and dark terminal themes. This centralized approach
// ensures consistent theming across all command outputs.
//
// Style names are used as XML-like tags in templates:
//
//	<Success>Operation completed</Success>
//	<Handler>symlink</Handler>
//
// See pkg/output/doc.go for the complete rendering pipeline documentation.
package styles

import (
	_ "embed"
	"fmt"
	"os"
	"path/filepath"

	"github.com/charmbracelet/lipgloss"
	"gopkg.in/yaml.v3"
)

// ColorDef represents an adaptive color definition in YAML
type ColorDef struct {
	Light string `yaml:"light"`
	Dark  string `yaml:"dark"`
}

// StyleDef represents a style definition in YAML
type StyleDef struct {
	Bold         bool   `yaml:"bold,omitempty"`
	Italic       bool   `yaml:"italic,omitempty"`
	Underline    bool   `yaml:"underline,omitempty"`
	Foreground   string `yaml:"foreground,omitempty"`
	Background   string `yaml:"background,omitempty"`
	Width        int    `yaml:"width,omitempty"`
	Align        string `yaml:"align,omitempty"`
	MarginLeft   int    `yaml:"marginLeft,omitempty"`
	MarginBottom int    `yaml:"marginBottom,omitempty"`
	MarginTop    int    `yaml:"marginTop,omitempty"`
	PaddingLeft  int    `yaml:"paddingLeft,omitempty"`
	PaddingRight int    `yaml:"paddingRight,omitempty"`
}

// Config represents the complete styles configuration
type Config struct {
	Colors map[string]ColorDef `yaml:"colors"`
	Styles map[string]StyleDef `yaml:"styles"`
}

// StyleRegistry maps semantic names to lipgloss styles
var StyleRegistry map[string]lipgloss.Style

// Adaptive colors loaded from YAML
var colors map[string]lipgloss.AdaptiveColor

//go:embed styles.yaml
var embeddedStyles []byte

// getLogger returns a logger for the styles package

func init() {
	// Try to load styles with multiple fallbacks
	var err error

	// 1. Try embedded data first (this should always work in production)
	if len(embeddedStyles) > 0 {
		if err = LoadStylesFromData(embeddedStyles); err == nil {
			return
		}
	}

	// 2. In development, try loading from file
	if devPath := findDevStylesPath(); devPath != "" {
		if err = LoadStyles(devPath); err == nil {
			return
		}
	}

	// 3. If all fails, try some common paths as last resort
	for _, path := range getCommonStylesPaths() {
		if err = LoadStyles(path); err == nil {
			return
		}
	}

	// If we get here, we couldn't load styles from anywhere
	// Use default styles instead of panicking
	initDefaultStyles()
}

// findDevStylesPath looks for styles.yaml in development environment
func findDevStylesPath() string {
	// Check if we're in development by looking for the file
	devPaths := []string{
		"pkg/ui/output/styles/styles.yaml", // From repo root
		"styles.yaml",                      // From package directory
	}

	for _, path := range devPaths {
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}

	return ""
}

// getCommonStylesPaths returns common installation paths for styles.yaml
func getCommonStylesPaths() []string {
	paths := []string{
		// Homebrew locations
		"/opt/homebrew/share/dodot/styles/styles.yaml", // Apple Silicon Mac
		"/usr/local/share/dodot/styles/styles.yaml",    // Intel Mac
	}

	// Add paths relative to binary
	if exePath, err := os.Executable(); err == nil {
		exeDir := filepath.Dir(exePath)
		paths = append(paths,
			filepath.Join(exeDir, "..", "share", "dodot", "styles", "styles.yaml"),
			filepath.Join(exeDir, "..", "share", "styles", "styles.yaml"),
			filepath.Join(exeDir, "styles", "styles.yaml"),
		)
	}

	return paths
}

// initDefaultStyles initializes a minimal set of default styles
// This ensures the program can run even if styles.yaml is missing
func initDefaultStyles() {
	colors = make(map[string]lipgloss.AdaptiveColor)
	StyleRegistry = make(map[string]lipgloss.Style)

	// Define minimal styles to prevent crashes
	defaultStyle := lipgloss.NewStyle()
	for _, name := range []string{
		"Header", "Success", "Error", "Warning", "Info",
		"Bold", "Italic", "Muted", "Handler", "FilePath",
	} {
		StyleRegistry[name] = defaultStyle
	}
}

// LoadStyles loads style configuration from a YAML file
func LoadStyles(path string) error {
	// Read YAML file
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Errorf("failed to read styles file %s: %w", path, err)
	}

	// Parse YAML configuration
	var config Config
	if err := yaml.Unmarshal(data, &config); err != nil {
		return fmt.Errorf("failed to parse styles.yaml: %w", err)
	}

	// Initialize colors
	colors = make(map[string]lipgloss.AdaptiveColor)
	for name, def := range config.Colors {
		colors[name] = lipgloss.AdaptiveColor{
			Light: def.Light,
			Dark:  def.Dark,
		}
	}

	// Initialize style registry
	StyleRegistry = make(map[string]lipgloss.Style)
	for name, def := range config.Styles {
		style := buildStyle(def)
		StyleRegistry[name] = style
	}

	return nil
}

// LoadStylesFromData loads style configuration from byte data
func LoadStylesFromData(data []byte) error {
	// Parse YAML configuration
	var config Config
	if err := yaml.Unmarshal(data, &config); err != nil {
		return fmt.Errorf("failed to parse styles data: %w", err)
	}

	// Initialize colors
	colors = make(map[string]lipgloss.AdaptiveColor)
	for name, def := range config.Colors {
		colors[name] = lipgloss.AdaptiveColor{
			Light: def.Light,
			Dark:  def.Dark,
		}
	}

	// Initialize style registry
	StyleRegistry = make(map[string]lipgloss.Style)
	for name, def := range config.Styles {
		style := buildStyle(def)
		StyleRegistry[name] = style
	}

	return nil
}

// buildStyle constructs a lipgloss style from a style definition
func buildStyle(def StyleDef) lipgloss.Style {
	style := lipgloss.NewStyle()

	// Apply text formatting
	if def.Bold {
		style = style.Bold(true)
	}
	if def.Italic {
		style = style.Italic(true)
	}
	if def.Underline {
		style = style.Underline(true)
	}

	// Apply colors
	if def.Foreground != "" {
		if color, ok := colors[def.Foreground]; ok {
			style = style.Foreground(color)
		}
	}
	if def.Background != "" {
		if color, ok := colors[def.Background]; ok {
			style = style.Background(color)
		}
	}

	// Apply layout
	if def.Width > 0 {
		style = style.Width(def.Width)
	}
	if def.Align != "" {
		switch def.Align {
		case "left":
			style = style.Align(lipgloss.Left)
		case "center":
			style = style.Align(lipgloss.Center)
		case "right":
			style = style.Align(lipgloss.Right)
		}
	}

	// Apply spacing
	if def.MarginLeft > 0 {
		style = style.MarginLeft(def.MarginLeft)
	}
	if def.MarginBottom > 0 {
		style = style.MarginBottom(def.MarginBottom)
	}
	if def.MarginTop > 0 {
		style = style.MarginTop(def.MarginTop)
	}
	if def.PaddingLeft > 0 || def.PaddingRight > 0 {
		style = style.Padding(0, def.PaddingRight, 0, def.PaddingLeft)
	}

	return style
}

// GetStyle safely retrieves a style from the registry
func GetStyle(name string) lipgloss.Style {
	if style, ok := StyleRegistry[name]; ok {
		return style
	}
	// Return a default style if not found
	return lipgloss.NewStyle()
}

// MergeStyles combines multiple styles
func MergeStyles(styles ...string) lipgloss.Style {
	result := lipgloss.NewStyle()
	for _, name := range styles {
		result = result.Inherit(GetStyle(name))
	}
	return result
}
