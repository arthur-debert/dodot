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
	"fmt"
	"os"
	"path/filepath"
	"runtime"

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

// defaultStylesPath is set during init based on source file location
var defaultStylesPath string

// getLogger returns a logger for the styles package

func init() {
	// Try to determine the path to styles.yaml
	// This handles both runtime and test scenarios

	// Strategy 1: Use runtime.Caller to find this source file
	_, filename, _, ok := runtime.Caller(0)
	if ok {
		// We know this file is in pkg/output/styles/
		dir := filepath.Dir(filename)
		stylesPath := filepath.Join(dir, "styles.yaml")
		if _, err := os.Stat(stylesPath); err == nil {
			defaultStylesPath = stylesPath
		}
	}

	// Strategy 2: Check executable path
	if defaultStylesPath == "" {
		exePath, err := os.Executable()
		if err == nil {
			// Try paths relative to the binary
			exeDir := filepath.Dir(exePath)
			possiblePaths := []string{
				// Same directory as binary
				filepath.Join(exeDir, "styles.yaml"),
				// Development: binary in bin/, styles in pkg/
				filepath.Join(exeDir, "..", "pkg", "output", "styles", "styles.yaml"),
				// Alternative development path
				filepath.Join(exeDir, "..", "..", "pkg", "output", "styles", "styles.yaml"),
			}

			for _, path := range possiblePaths {
				if _, err = os.Stat(path); err == nil {
					defaultStylesPath = path
					break
				}
			}
		}
	}

	// Strategy 3: Try working directory paths
	if defaultStylesPath == "" {
		workingDirPaths := []string{
			// Development/test path (when running from repo root)
			"pkg/output/styles/styles.yaml",
			// Test path (when running from package directory)
			"styles.yaml",
			// Test path when running go test from a subdirectory
			"../styles/styles.yaml",
			"../../styles/styles.yaml",
			"../../../pkg/output/styles/styles.yaml",
		}

		for _, path := range workingDirPaths {
			if _, err := os.Stat(path); err == nil {
				defaultStylesPath = path
				break
			}
		}
	}

	if defaultStylesPath == "" {
		// Last resort - assume we're in development
		defaultStylesPath = "pkg/output/styles/styles.yaml"
	}

	// Load styles configuration from file
	if err := LoadStyles(defaultStylesPath); err != nil {
		panic(fmt.Sprintf("failed to load styles: %v", err))
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
