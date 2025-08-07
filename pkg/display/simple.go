package display

import (
	"fmt"
	"io"

	"github.com/arthur-debert/dodot/pkg/types"
)

// SimpleRenderer provides minimal text output for dodot commands
type SimpleRenderer struct {
	writer io.Writer
}

// NewSimpleRenderer creates a new simple renderer
func NewSimpleRenderer(w io.Writer) *SimpleRenderer {
	return &SimpleRenderer{
		writer: w,
	}
}

// Render outputs the DisplayResult in a simple text format
func (r *SimpleRenderer) Render(result *types.DisplayResult) error {
	if result == nil {
		return nil
	}

	// Command header
	if _, err := fmt.Fprintf(r.writer, "%s", result.Command); err != nil {
		return err
	}
	if result.DryRun {
		if _, err := fmt.Fprint(r.writer, " (dry run)"); err != nil {
			return err
		}
	}
	if _, err := fmt.Fprintln(r.writer); err != nil {
		return err
	}

	// For commands with no packs
	if len(result.Packs) == 0 {
		if _, err := fmt.Fprintln(r.writer, "No packs to process"); err != nil {
			return err
		}
		return nil
	}

	// Render each pack
	for _, pack := range result.Packs {
		if err := r.renderPack(pack); err != nil {
			return err
		}
	}

	return nil
}

// renderPack renders a single pack
func (r *SimpleRenderer) renderPack(pack types.DisplayPack) error {
	// Pack header with status indicator
	statusChar := r.getStatusChar(pack.Status)
	if _, err := fmt.Fprintf(r.writer, "\n%s %s\n", statusChar, pack.Name); err != nil {
		return err
	}

	// Render files
	if len(pack.Files) == 0 {
		if _, err := fmt.Fprintln(r.writer, "  (no files)"); err != nil {
			return err
		}
		return nil
	}

	for _, file := range pack.Files {
		if err := r.renderFile(file); err != nil {
			return err
		}
	}

	return nil
}

// renderFile renders a single file
func (r *SimpleRenderer) renderFile(file types.DisplayFile) error {
	statusChar := r.getStatusChar(file.Status)

	// Format: status powerup : path : message
	_, err := fmt.Fprintf(r.writer, "  %s %-12s : %-30s : %s\n",
		statusChar,
		file.PowerUp,
		truncatePath(file.Path, 30),
		file.Message)
	return err
}

// getStatusChar returns a simple character to represent status
func (r *SimpleRenderer) getStatusChar(status string) string {
	switch status {
	case "success":
		return "✓"
	case "error", "alert":
		return "✗"
	case "queue":
		return "•"
	default:
		return " "
	}
}

// truncatePath truncates a path to fit within maxLen characters
func truncatePath(path string, maxLen int) string {
	if len(path) <= maxLen {
		return path
	}

	// Show beginning and end of path
	if maxLen < 10 {
		return path[:maxLen]
	}

	// Reserve 3 chars for "..."
	availableChars := maxLen - 3
	startChars := availableChars / 2
	endChars := availableChars - startChars

	return path[:startChars] + "..." + path[len(path)-endChars:]
}

// RenderExecutionContext is a convenience method that transforms and renders an ExecutionContext
func (r *SimpleRenderer) RenderExecutionContext(ctx *types.ExecutionContext) error {
	if ctx == nil {
		return nil
	}

	displayResult := ctx.ToDisplayResult()
	return r.Render(displayResult)
}
