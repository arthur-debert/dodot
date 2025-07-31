package dodot

import (
	"os"
	"strings"
	"text/template"

	"github.com/mattn/go-isatty"
	"github.com/pterm/pterm"
	"github.com/spf13/cobra"
)

// formatBold returns the string formatted as bold using pterm
func formatBold(s string) string {
	// Only apply formatting if output is a terminal
	if !isatty.IsTerminal(os.Stdout.Fd()) && !isatty.IsCygwinTerminal(os.Stdout.Fd()) {
		return s
	}
	return pterm.Bold.Sprint(s)
}

// formatUpper returns the string in uppercase
func formatUpper(s string) string {
	return strings.ToUpper(s)
}

// formatBoldUpper returns the string in uppercase and bold
func formatBoldUpper(s string) string {
	upper := strings.ToUpper(s)
	// Only apply formatting if output is a terminal
	if !isatty.IsTerminal(os.Stdout.Fd()) && !isatty.IsCygwinTerminal(os.Stdout.Fd()) {
		return upper
	}
	return pterm.Bold.Sprint(upper)
}

// initTemplateFormatting adds custom formatting functions to Cobra templates
func initTemplateFormatting() {
	cobra.AddTemplateFuncs(template.FuncMap{
		"bold":      formatBold,
		"upper":     formatUpper,
		"boldUpper": formatBoldUpper,
	})
}
