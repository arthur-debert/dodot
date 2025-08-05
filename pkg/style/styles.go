package style

import (
	"github.com/charmbracelet/lipgloss"
)

// Base styles
var (
	// Headers and titles
	TitleStyle = lipgloss.NewStyle().
			Foreground(HeadingColor).
			Bold(true).
			MarginBottom(1)

	SubtitleStyle = lipgloss.NewStyle().
			Foreground(HeadingColor).
			Bold(true)

	// Text styles
	NormalStyle = lipgloss.NewStyle().
			Foreground(TextColor)

	MutedStyle = lipgloss.NewStyle().
			Foreground(MutedColor)

	// Status styles
	SuccessStyle = lipgloss.NewStyle().
			Foreground(SuccessColor).
			Bold(true)

	ErrorStyle = lipgloss.NewStyle().
			Foreground(ErrorColor).
			Bold(true)

	WarningStyle = lipgloss.NewStyle().
			Foreground(WarningColor).
			Bold(true)

	InfoStyle = lipgloss.NewStyle().
			Foreground(InfoColor)

	// Box and container styles
	BoxStyle = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(BorderColor).
			Padding(1, 2)

	// List styles
	ListItemStyle = lipgloss.NewStyle().
			PaddingLeft(2)

	// Code and path styles
	CodeStyle = lipgloss.NewStyle().
			Foreground(PrimaryColor).
			Background(SurfaceColor).
			Padding(0, 1)

	PathStyle = lipgloss.NewStyle().
			Foreground(SecondaryColor).
			Italic(true)
)

// PowerUp styles
var (
	SymlinkStyle = lipgloss.NewStyle().
			Foreground(SymlinkColor).
			Bold(true)

	ProfileStyle = lipgloss.NewStyle().
			Foreground(ProfileColor).
			Bold(true)

	InstallScriptStyle = lipgloss.NewStyle().
				Foreground(InstallScriptColor).
				Bold(true)

	HomebrewStyle = lipgloss.NewStyle().
			Foreground(HomebrewColor).
			Bold(true)
)

// Operation indicator styles
var (
	SuccessIndicator  = SuccessStyle.Render("✓")
	ErrorIndicator    = ErrorStyle.Render("✗")
	WarningIndicator  = WarningStyle.Render("!")
	InfoIndicator     = InfoStyle.Render("•")
	PendingIndicator  = MutedStyle.Render("○")
	ProgressIndicator = InfoStyle.Render("⟳")
)

// Helper functions
func Indent(s string, level int) string {
	return lipgloss.NewStyle().PaddingLeft(level * 2).Render(s)
}

func Bold(s string) string {
	return lipgloss.NewStyle().Bold(true).Render(s)
}

func Italic(s string) string {
	return lipgloss.NewStyle().Italic(true).Render(s)
}

func Underline(s string) string {
	return lipgloss.NewStyle().Underline(true).Render(s)
}
