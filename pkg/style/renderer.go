package style

import (
	"fmt"
	"strings"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/pterm/pterm"
)

// Renderer defines the interface for rendering various output types
type Renderer interface {
	RenderPackList(packs []types.PackInfo) string
	RenderOperations(ops []types.Operation) string
	RenderError(err error) string
	RenderProgress(current, total int, message string) string
	RenderPackStatuses(packs []PackStatus) string
}

// TerminalRenderer implements Renderer with rich terminal output
type TerminalRenderer struct {
	width int
}

// NewTerminalRenderer creates a new terminal renderer
func NewTerminalRenderer() *TerminalRenderer {
	return &TerminalRenderer{
		width: 80, // Default width, can be updated
	}
}

// SetWidth updates the terminal width for rendering
func (r *TerminalRenderer) SetWidth(width int) {
	r.width = width
}

// RenderPackList renders a list of packs
func (r *TerminalRenderer) RenderPackList(packs []types.PackInfo) string {
	if len(packs) == 0 {
		return MutedStyle.Sprint("No packs found")
	}

	var result strings.Builder
	result.WriteString(TitleStyle.Sprint("Available Packs") + "\n\n")

	for _, pack := range packs {
		// Pack name with icon
		packLine := fmt.Sprintf("%s %s", pterm.Info.Prefix.Text, Bold(pack.Name))
		result.WriteString(packLine + "\n")

		// Pack path (indented and muted)
		if pack.Path != "" {
			pathLine := Indent(MutedStyle.Sprint(pack.Path), 1)
			result.WriteString(pathLine + "\n")
		}

		// Add spacing between packs
		result.WriteString("\n")
	}

	return strings.TrimRight(result.String(), "\n")
}

// RenderOperations renders a list of operations
func (r *TerminalRenderer) RenderOperations(ops []types.Operation) string {
	if len(ops) == 0 {
		return MutedStyle.Sprint("No operations to perform")
	}

	var result strings.Builder

	// For now, render operations without grouping by pack
	// since Operation struct doesn't have a Pack field
	result.WriteString(TitleStyle.Sprint("Operations") + "\n\n")

	// Render each operation
	for _, op := range ops {
		opLine := r.renderOperation(op)
		result.WriteString(opLine + "\n")
	}

	return strings.TrimRight(result.String(), "\n")
}

// renderOperation renders a single operation
func (r *TerminalRenderer) renderOperation(op types.Operation) string {
	// Choose indicator based on operation status
	var indicator string
	switch op.Status {
	case types.StatusReady:
		indicator = PendingIndicator
	case types.StatusSkipped:
		indicator = InfoIndicator
	case types.StatusConflict:
		indicator = WarningIndicator
	case types.StatusError:
		indicator = ErrorIndicator
	default:
		indicator = InfoIndicator
	}

	// Choose style based on operation type
	var typeStyle *pterm.Style
	var typeName string
	switch op.Type {
	case types.OperationCreateSymlink:
		typeStyle = SymlinkStyle
		typeName = "symlink"
	case types.OperationWriteFile:
		typeStyle = ProfileStyle
		typeName = "profile"
	case types.OperationExecute:
		typeStyle = InstallScriptStyle
		typeName = "execute"
	default:
		typeStyle = pterm.Info.MessageStyle
		typeName = string(op.Type)
	}

	// Format operation
	opType := typeStyle.Sprint(typeName)

	// Build operation description
	var desc string
	if op.Source != "" && op.Target != "" {
		desc = fmt.Sprintf("%s → %s",
			PathStyle.Sprint(op.Source),
			PathStyle.Sprint(op.Target))
	} else if op.Description != "" {
		desc = op.Description
	}

	return fmt.Sprintf("%s %s %s", indicator, opType, desc)
}

// RenderError renders an error message
func (r *TerminalRenderer) RenderError(err error) string {
	if err == nil {
		return ""
	}

	// Check if it's a dodot error with code
	if dodotErr, ok := err.(interface{ Code() string }); ok {
		return fmt.Sprintf("%s Error [%s]: %s",
			pterm.Error.Prefix.Text,
			pterm.Error.MessageStyle.Sprint(dodotErr.Code()),
			err.Error())
	}

	// Generic error
	return fmt.Sprintf("%s %s", pterm.Error.Prefix.Text, pterm.Error.MessageStyle.Sprint(err.Error()))
}

// RenderProgress renders a progress indicator
func (r *TerminalRenderer) RenderProgress(current, total int, message string) string {
	// Progress bar
	percentage := float64(current) / float64(total)
	barWidth := 20
	filled := int(percentage * float64(barWidth))

	bar := strings.Repeat("█", filled) + strings.Repeat("░", barWidth-filled)

	return fmt.Sprintf("%s [%s] %d/%d %s",
		ProgressIndicator,
		pterm.Info.MessageStyle.Sprint(bar),
		current,
		total,
		message)
}

// RenderPackStatuses renders multiple pack statuses
func (r *TerminalRenderer) RenderPackStatuses(packs []PackStatus) string {
	if len(packs) == 0 {
		return MutedStyle.Sprint("No packs to display")
	}

	var result strings.Builder

	for i, pack := range packs {
		result.WriteString(RenderPackStatus(pack))
		// Add spacing between packs except for the last one
		if i < len(packs)-1 {
			result.WriteString("\n\n")
		}
	}

	return result.String()
}

// PlainRenderer implements Renderer with plain text output (no styling)
type PlainRenderer struct{}

// NewPlainRenderer creates a new plain text renderer
func NewPlainRenderer() *PlainRenderer {
	return &PlainRenderer{}
}

// RenderPackList renders a plain list of packs
func (r *PlainRenderer) RenderPackList(packs []types.PackInfo) string {
	if len(packs) == 0 {
		return "No packs found"
	}

	var result strings.Builder
	result.WriteString("Available Packs:\n")

	for _, pack := range packs {
		result.WriteString(fmt.Sprintf("  - %s\n", pack.Name))
	}

	return strings.TrimRight(result.String(), "\n")
}

// FIXME: ARCHITECTURAL PROBLEM - UI should NOT render Operation types!
// User-facing display should show Pack->PowerUp->File status, not operations.
// Operations are internal implementation details. Users understand:
// - "vim pack: symlink .vimrc -> ~/.vimrc" (PowerUp level)
// NOT "Operation: CreateSymlink source=/path target=/path" (Operation level)
// See docs/design/display.txxt
// RenderOperations renders plain operations
func (r *PlainRenderer) RenderOperations(ops []types.Operation) string {
	if len(ops) == 0 {
		return "No operations to perform"
	}

	var result strings.Builder

	for _, op := range ops {
		result.WriteString(fmt.Sprintf("%s: %s\n", op.Type, op.Description))
	}

	return strings.TrimRight(result.String(), "\n")
}

// RenderError renders a plain error message
func (r *PlainRenderer) RenderError(err error) string {
	if err == nil {
		return ""
	}
	return fmt.Sprintf("Error: %s", err.Error())
}

// RenderProgress renders plain progress
func (r *PlainRenderer) RenderProgress(current, total int, message string) string {
	return fmt.Sprintf("Progress: %d/%d - %s", current, total, message)
}

// RenderPackStatuses renders multiple pack statuses in plain text
func (r *PlainRenderer) RenderPackStatuses(packs []PackStatus) string {
	if len(packs) == 0 {
		return "No packs to display"
	}

	var result strings.Builder

	for i, pack := range packs {
		result.WriteString(r.renderPlainPackStatus(pack))
		// Add spacing between packs except for the last one
		if i < len(packs)-1 {
			result.WriteString("\n\n")
		}
	}

	return result.String()
}

// renderPlainPackStatus renders a single pack status in plain text
func (r *PlainRenderer) renderPlainPackStatus(ps PackStatus) string {
	var result strings.Builder

	// Pack header
	result.WriteString(ps.Name + ":\n")

	// Special case: ignored directory
	if ps.IsIgnored {
		result.WriteString("    .dodotignore : dodot is ignoring this dir\n")
		return result.String()
	}

	// Config file if present
	if ps.HasConfig {
		result.WriteString("    config      : .dodot.toml : dodot config file found\n")
	}

	// File statuses
	for _, fs := range ps.Files {
		powerUp := fmt.Sprintf("%-10s", fs.PowerUp)
		filePath := fmt.Sprintf("%-15s", fs.FilePath)

		// Build status message
		var statusMsg string
		if verbs, ok := PowerUpVerbs[fs.PowerUp]; ok {
			switch fs.Status {
			case StatusSuccess:
				statusMsg = fmt.Sprintf("%s %s", verbs.Past, fs.Target)
				if fs.Date != "" {
					statusMsg += fmt.Sprintf(" (%s)", fs.Date)
				}
			case StatusQueue:
				statusMsg = fmt.Sprintf("%s %s", verbs.Future, fs.Target)
			case StatusError:
				statusMsg = fmt.Sprintf("failed to %s %s", strings.ToLower(fs.PowerUp), fs.Target)
			}
		}

		result.WriteString(fmt.Sprintf("    %s : %s : %s\n", powerUp, filePath, statusMsg))
	}

	return strings.TrimRight(result.String(), "\n")
}
