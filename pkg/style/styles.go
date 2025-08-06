package style

import (
	"github.com/charmbracelet/lipgloss"
	"github.com/pterm/pterm"
)

// Initialize pterm styles with our custom colors
func init() {
	// Configure pterm with our theme colors
	pterm.Success.Prefix = pterm.Prefix{
		Text:  "✓",
		Style: pterm.NewStyle(pterm.FgGreen, pterm.Bold),
	}

	pterm.Error.Prefix = pterm.Prefix{
		Text:  "✗",
		Style: pterm.NewStyle(pterm.FgRed, pterm.Bold),
	}

	pterm.Warning.Prefix = pterm.Prefix{
		Text:  "!",
		Style: pterm.NewStyle(pterm.FgYellow, pterm.Bold),
	}

	pterm.Info.Prefix = pterm.Prefix{
		Text:  "•",
		Style: pterm.NewStyle(pterm.FgCyan),
	}
}

// Create pterm styles for our use cases
var (
	// Text styles using pterm
	TitleStyle    = pterm.NewStyle(pterm.Bold)
	SubtitleStyle = pterm.NewStyle(pterm.Bold)
	MutedStyle    = pterm.NewStyle(pterm.FgGray)

	// PowerUp styles with custom colors
	SymlinkStyle       = pterm.NewStyle(pterm.FgLightBlue, pterm.Bold)
	ProfileStyle       = pterm.NewStyle(pterm.FgMagenta, pterm.Bold)
	InstallScriptStyle = pterm.NewStyle(pterm.FgYellow, pterm.Bold)
	HomebrewStyle      = pterm.NewStyle(pterm.FgGreen, pterm.Bold)
	ConfigStyle        = pterm.NewStyle(pterm.FgCyan) // For .dodot.toml files

	// Path style
	PathStyle = pterm.NewStyle(pterm.FgGray, pterm.Italic)
)

// Operation indicators - use the Unicode symbols directly
var (
	SuccessIndicator  = "✓"
	ErrorIndicator    = "✗"
	WarningIndicator  = "!"
	InfoIndicator     = "•"
	PendingIndicator  = "○"
	ProgressIndicator = "⟳"
)

// Helper functions using pterm
func Indent(s string, level int) string {
	// Use spaces for indentation
	indent := ""
	for i := 0; i < level*2; i++ {
		indent += " "
	}
	return indent + s
}

func Bold(s string) string {
	return pterm.Bold.Sprint(s)
}

func Italic(s string) string {
	return pterm.Italic.Sprint(s)
}

func Underline(s string) string {
	return pterm.Underscore.Sprint(s)
}

// Lipgloss styles for more complex layouts (boxes, etc)
var (
	BoxStyle = lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(BorderColor).
		Padding(1, 2)
)
