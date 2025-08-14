// Package styles defines the visual styling for dodot's terminal output.
//
// All styles use semantic names and adaptive colors that automatically
// adjust to light and dark terminal themes. This centralized approach
// ensures consistent theming across all command outputs.
//
// Style names are used as XML-like tags in templates:
//
//	<Success>Operation completed</Success>
//	<PowerUp>symlink</PowerUp>
//
// See pkg/output/doc.go for the complete rendering pipeline documentation.
package styles

import (
	_ "embed"
	"fmt"

	"github.com/charmbracelet/lipgloss"
	"gopkg.in/yaml.v3"
)

//go:embed styles.yaml
var stylesYAML []byte

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

func init() {
	// Parse YAML configuration
	var config Config
	if err := yaml.Unmarshal(stylesYAML, &config); err != nil {
		panic(fmt.Sprintf("failed to parse styles.yaml: %v", err))
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
