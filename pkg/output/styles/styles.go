package styles

import (
	"github.com/charmbracelet/lipgloss"
)

// Semantic colors using AdaptiveColor for automatic light/dark mode support
var (
	// Base colors
	ColorPrimary   = lipgloss.AdaptiveColor{Light: "#005F87", Dark: "#87CEEB"}
	ColorSecondary = lipgloss.AdaptiveColor{Light: "#875F00", Dark: "#FFD700"}
	ColorMuted     = lipgloss.AdaptiveColor{Light: "#6C6C6C", Dark: "#878787"}

	// Status colors
	ColorSuccess = lipgloss.AdaptiveColor{Light: "#008700", Dark: "#00FF00"}
	ColorError   = lipgloss.AdaptiveColor{Light: "#D70000", Dark: "#FF5555"}
	ColorWarning = lipgloss.AdaptiveColor{Light: "#AF8700", Dark: "#FFFF00"}
	ColorInfo    = lipgloss.AdaptiveColor{Light: "#0087AF", Dark: "#00AFFF"}
	ColorQueued  = lipgloss.AdaptiveColor{Light: "#5F5F87", Dark: "#8787AF"}
	ColorIgnored = lipgloss.AdaptiveColor{Light: "#9E9E9E", Dark: "#626262"}

	// Background colors
	ColorSuccessBg = lipgloss.AdaptiveColor{Light: "#D7FFD7", Dark: "#003300"}
	ColorErrorBg   = lipgloss.AdaptiveColor{Light: "#FFD7D7", Dark: "#330000"}
	ColorWarningBg = lipgloss.AdaptiveColor{Light: "#FFFFD7", Dark: "#333300"}
)

// StyleRegistry maps semantic names to lipgloss styles
var StyleRegistry = map[string]lipgloss.Style{
	// Headers
	"Header":        lipgloss.NewStyle().Bold(true).Foreground(ColorPrimary),
	"SubHeader":     lipgloss.NewStyle().Foreground(ColorSecondary),
	"PackHeader":    lipgloss.NewStyle().Bold(true).Foreground(ColorPrimary).MarginBottom(1),
	"CommandHeader": lipgloss.NewStyle().Bold(true).Foreground(ColorPrimary).MarginTop(1),

	// Status indicators
	"Success": lipgloss.NewStyle().Foreground(ColorSuccess).Bold(true),
	"Error":   lipgloss.NewStyle().Foreground(ColorError).Bold(true),
	"Warning": lipgloss.NewStyle().Foreground(ColorWarning).Bold(true),
	"Info":    lipgloss.NewStyle().Foreground(ColorInfo),
	"Queued":  lipgloss.NewStyle().Foreground(ColorQueued),
	"Ignored": lipgloss.NewStyle().Foreground(ColorIgnored).Italic(true),

	// Status with backgrounds (for emphasis)
	"SuccessBadge": lipgloss.NewStyle().Foreground(ColorSuccess).Background(ColorSuccessBg).Padding(0, 1),
	"ErrorBadge":   lipgloss.NewStyle().Foreground(ColorError).Background(ColorErrorBg).Padding(0, 1),
	"WarningBadge": lipgloss.NewStyle().Foreground(ColorWarning).Background(ColorWarningBg).Padding(0, 1),

	// Text styles
	"Bold":        lipgloss.NewStyle().Bold(true),
	"Italic":      lipgloss.NewStyle().Italic(true),
	"Underline":   lipgloss.NewStyle().Underline(true),
	"Muted":       lipgloss.NewStyle().Foreground(ColorMuted),
	"MutedItalic": lipgloss.NewStyle().Foreground(ColorMuted).Italic(true),

	// File and path styles
	"PowerUp":    lipgloss.NewStyle().Foreground(ColorSecondary).Bold(true),
	"FilePath":   lipgloss.NewStyle().Foreground(ColorPrimary),
	"ConfigFile": lipgloss.NewStyle().Foreground(ColorInfo).Italic(true),
	"Override":   lipgloss.NewStyle().Foreground(ColorWarning).Bold(true),

	// Layout and spacing
	"Indent":       lipgloss.NewStyle().MarginLeft(2),
	"DoubleIndent": lipgloss.NewStyle().MarginLeft(4),
	"Section":      lipgloss.NewStyle().MarginBottom(1),

	// Special elements
	"Timestamp":    lipgloss.NewStyle().Foreground(ColorMuted).Italic(true),
	"DryRunBanner": lipgloss.NewStyle().Foreground(ColorWarning).Background(ColorWarningBg).Bold(true).Padding(0, 2),
	"NoContent":    lipgloss.NewStyle().Foreground(ColorMuted).Italic(true),

	// Table elements
	"TableHeader":    lipgloss.NewStyle().Bold(true).Underline(true).Foreground(ColorPrimary),
	"TableCell":      lipgloss.NewStyle(),
	"TableSeparator": lipgloss.NewStyle().Foreground(ColorMuted),
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
