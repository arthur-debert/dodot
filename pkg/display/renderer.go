package display

import (
	"fmt"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/style"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/pterm/pterm"
)

// Renderer defines the interface for rendering display results
type Renderer interface {
	// RenderCommandResult renders the complete command result
	RenderCommandResult(result CommandResult) string

	// RenderPackResult renders a single pack result
	RenderPackResult(pack PackResult) string

	// RenderFileResult renders a single file result
	RenderFileResult(file FileResult) string

	// RenderSummary renders the command summary
	RenderSummary(summary Summary) string
}

// RichRenderer implements Renderer with rich terminal output
// This renderer produces the three-column format specified in the design
type RichRenderer struct {
	// Column widths for the three-column layout
	actionWidth  int
	pathWidth    int
	messageWidth int
}

// NewRichRenderer creates a new rich terminal renderer
func NewRichRenderer() *RichRenderer {
	return &RichRenderer{
		actionWidth:  10, // As per design spec
		pathWidth:    15, // As per design spec
		messageWidth: 30,
	}
}

// RenderCommandResult renders the complete command result
func (r *RichRenderer) RenderCommandResult(result CommandResult) string {
	var output strings.Builder

	// Command header
	// Capitalize first letter of command
	headerText := result.Command
	if len(headerText) > 0 {
		headerText = strings.ToUpper(headerText[:1]) + headerText[1:]
	}
	if result.DryRun {
		headerText += " (dry run)"
	}
	output.WriteString(style.TitleStyle.Sprint(headerText) + "\n\n")

	// Render each pack
	for i, pack := range result.Packs {
		output.WriteString(r.RenderPackResult(pack))
		if i < len(result.Packs)-1 {
			output.WriteString("\n\n")
		}
	}

	// Render summary
	if len(result.Packs) > 0 {
		output.WriteString("\n\n")
		output.WriteString(r.RenderSummary(result.Summary))
	}

	return output.String()
}

// RenderPackResult renders a single pack result
func (r *RichRenderer) RenderPackResult(pack PackResult) string {
	var output strings.Builder

	// Pack header with status indicator
	statusIndicator := r.getPackStatusIndicator(pack.Status)
	packName := pack.Name

	// Apply ALERT styling for packs with errors
	if pack.Status == types.ExecutionStatusError {
		// Use ALERT style for pack name when it has errors
		packName = style.StatusStyle(style.StatusAlert).Sprint(packName)
	} else {
		packName = style.Bold(packName)
	}

	packHeader := fmt.Sprintf("%s %s", statusIndicator, packName)
	output.WriteString(packHeader + "\n")

	// Pack description if available
	if pack.Description != "" {
		output.WriteString(style.Indent(style.MutedStyle.Sprint(pack.Description), 1) + "\n")
	}

	// Group files by PowerUp for better organization
	groups := pack.GroupFilesByPowerUp()

	// Render each group
	first := true
	for _, files := range groups {
		if !first {
			output.WriteString("\n")
		}
		first = false

		// Render files in this group
		for _, file := range files {
			output.WriteString(style.Indent(r.RenderFileResult(file), 1) + "\n")
		}
	}

	// Pack summary if there were operations
	if pack.TotalOperations > 0 {
		summary := r.renderPackSummary(pack)
		output.WriteString(style.Indent(style.MutedStyle.Sprint(summary), 1))
	}

	return strings.TrimRight(output.String(), "\n")
}

// RenderFileResult renders a single file result in three-column format
func (r *RichRenderer) RenderFileResult(file FileResult) string {
	// Column 1: Power-up name (not action verb) with appropriate styling
	actionStyle := r.getActionStyle(file.PowerUp)
	powerUpName := r.padRight(file.PowerUp, r.actionWidth)
	powerUp := actionStyle.Sprint(powerUpName)

	// Column 2: File path
	pathName := r.padRight(file.Path, r.pathWidth)
	path := style.PathStyle.Sprint(pathName)

	// Column 3: Status message
	message := file.Message

	// Add output indicator if command produced output
	if file.HasOutput() {
		message += " [output]"
	}

	// Use " : " separator as per design spec
	// Format: <power-up> : <file-path> : <status-message>
	return fmt.Sprintf("%s : %s : %s", powerUp, path, message)
}

// RenderSummary renders the command summary
func (r *RichRenderer) RenderSummary(summary Summary) string {
	var output strings.Builder

	output.WriteString(style.TitleStyle.Sprint("Summary") + "\n")

	// Overall statistics
	stats := []string{
		fmt.Sprintf("Total packs: %d", summary.TotalPacks),
		fmt.Sprintf("Total operations: %d", summary.TotalOperations),
	}

	// Add breakdown if there were operations
	if summary.TotalOperations > 0 {
		if summary.CompletedOperations > 0 {
			stats = append(stats, fmt.Sprintf("✓ Completed: %d", summary.CompletedOperations))
		}
		if summary.SkippedOperations > 0 {
			stats = append(stats, fmt.Sprintf("○ Skipped: %d", summary.SkippedOperations))
		}
		if summary.FailedOperations > 0 {
			stats = append(stats, fmt.Sprintf("✗ Failed: %d", summary.FailedOperations))
		}
	}

	for _, stat := range stats {
		output.WriteString(style.Indent(stat, 1) + "\n")
	}

	// Duration
	if summary.Duration > 0 {
		output.WriteString(style.Indent(fmt.Sprintf("Duration: %s", summary.Duration.Round(100*time.Millisecond)), 1))
	}

	return strings.TrimRight(output.String(), "\n")
}

// Helper methods

// getPackStatusIndicator returns the appropriate indicator for a pack status
func (r *RichRenderer) getPackStatusIndicator(status types.ExecutionStatus) string {
	switch status {
	case types.ExecutionStatusSuccess:
		return style.SuccessIndicator
	case types.ExecutionStatusPartial:
		return style.WarningIndicator
	case types.ExecutionStatusError:
		return style.ErrorIndicator
	case types.ExecutionStatusSkipped:
		return style.InfoIndicator
	default:
		return style.PendingIndicator
	}
}

// getActionStyle returns the appropriate style for a PowerUp action
func (r *RichRenderer) getActionStyle(powerUp string) *pterm.Style {
	switch powerUp {
	case "symlink":
		return style.SymlinkStyle
	case "shell_profile":
		return style.ProfileStyle
	case "add_path":
		return style.PathStyle
	case "homebrew", "install":
		return style.InstallScriptStyle
	case "template":
		return pterm.Info.MessageStyle
	case "config":
		// Config files use cyan style
		return style.ConfigStyle
	default:
		return pterm.Info.MessageStyle
	}
}

// renderPackSummary creates a summary line for a pack
func (r *RichRenderer) renderPackSummary(pack PackResult) string {
	parts := []string{}

	if pack.CompletedOperations > 0 {
		parts = append(parts, fmt.Sprintf("%d completed", pack.CompletedOperations))
	}
	if pack.SkippedOperations > 0 {
		parts = append(parts, fmt.Sprintf("%d skipped", pack.SkippedOperations))
	}
	if pack.FailedOperations > 0 {
		parts = append(parts, fmt.Sprintf("%d failed", pack.FailedOperations))
	}

	if len(parts) == 0 {
		return ""
	}

	return fmt.Sprintf("(%s)", strings.Join(parts, ", "))
}

// padRight pads a string to the specified width
func (r *RichRenderer) padRight(s string, width int) string {
	if len(s) > width {
		return s[:width-1] + "…"
	}
	if len(s) == width {
		return s
	}
	return s + strings.Repeat(" ", width-len(s))
}

// PlainRenderer implements Renderer with plain text output
type PlainRenderer struct{}

// NewPlainRenderer creates a new plain text renderer
func NewPlainRenderer() *PlainRenderer {
	return &PlainRenderer{}
}

// RenderCommandResult renders the command result as plain text
func (r *PlainRenderer) RenderCommandResult(result CommandResult) string {
	var output strings.Builder

	// Command header
	output.WriteString(strings.ToUpper(result.Command))
	if result.DryRun {
		output.WriteString(" (DRY RUN)")
	}
	output.WriteString("\n\n")

	// Render each pack
	for i, pack := range result.Packs {
		output.WriteString(r.RenderPackResult(pack))
		if i < len(result.Packs)-1 {
			output.WriteString("\n\n")
		}
	}

	// Summary
	if len(result.Packs) > 0 {
		output.WriteString("\n\n")
		output.WriteString(r.RenderSummary(result.Summary))
	}

	return output.String()
}

// RenderPackResult renders a pack result as plain text
func (r *PlainRenderer) RenderPackResult(pack PackResult) string {
	var output strings.Builder

	// Pack name
	output.WriteString(fmt.Sprintf("%s:\n", pack.Name))

	// Files
	for _, file := range pack.Files {
		output.WriteString("  " + r.RenderFileResult(file) + "\n")
	}

	return strings.TrimRight(output.String(), "\n")
}

// RenderFileResult renders a file result as plain text
func (r *PlainRenderer) RenderFileResult(file FileResult) string {
	// Format: <power-up> : <file-path> : <status-message>
	// Pad power-up to 10 chars and path to 15 chars
	powerUp := r.padRight(file.PowerUp, 10)
	path := r.padRight(file.Path, 15)

	return fmt.Sprintf("%s : %s : %s", powerUp, path, file.Message)
}

// padRight pads a string to the specified width
func (r *PlainRenderer) padRight(s string, width int) string {
	if len(s) > width {
		return s[:width-1] + "…"
	}
	if len(s) == width {
		return s
	}
	return s + strings.Repeat(" ", width-len(s))
}

// RenderSummary renders the summary as plain text
func (r *PlainRenderer) RenderSummary(summary Summary) string {
	var output strings.Builder

	output.WriteString("SUMMARY\n")
	output.WriteString(fmt.Sprintf("  Total packs: %d\n", summary.TotalPacks))
	output.WriteString(fmt.Sprintf("  Total operations: %d\n", summary.TotalOperations))

	if summary.TotalOperations > 0 {
		output.WriteString(fmt.Sprintf("  Completed: %d\n", summary.CompletedOperations))
		output.WriteString(fmt.Sprintf("  Skipped: %d\n", summary.SkippedOperations))
		output.WriteString(fmt.Sprintf("  Failed: %d\n", summary.FailedOperations))
	}

	return strings.TrimRight(output.String(), "\n")
}
