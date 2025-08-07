package display

import (
	"fmt"
	"io"

	"github.com/arthur-debert/dodot/pkg/types"
)

// TextRenderer provides minimal text output for dodot commands
type TextRenderer struct {
	writer io.Writer
}

// NewTextRenderer creates a new text renderer
func NewTextRenderer(w io.Writer) *TextRenderer {
	return &TextRenderer{
		writer: w,
	}
}

// Render outputs the DisplayResult in a simple text format
func (r *TextRenderer) Render(result *types.DisplayResult) error {
	if result == nil {
		return nil
	}

	// Command header
	commandHeader := result.Command
	if result.DryRun {
		commandHeader += " (dry run)"
	}
	if _, err := fmt.Fprintln(r.writer, commandHeader); err != nil {
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
func (r *TextRenderer) renderPack(pack types.DisplayPack) error {
	// Pack header - just the pack name with proper indentation
	if _, err := fmt.Fprintf(r.writer, "\n    %s:\n", pack.Name); err != nil {
		return err
	}

	// Render files
	if len(pack.Files) == 0 {
		if _, err := fmt.Fprintln(r.writer, "        (no files)"); err != nil {
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
func (r *TextRenderer) renderFile(file types.DisplayFile) error {
	// Three-column format matching display.txxt spec:
	// powerup : path : message
	// Use consistent spacing with left-aligned columns
	_, err := fmt.Fprintf(r.writer, "        %-12s : %-20s : %s\n",
		file.PowerUp,
		file.Path,
		file.Message)
	return err
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
func (r *TextRenderer) RenderExecutionContext(ctx *types.ExecutionContext) error {
	if ctx == nil {
		return nil
	}

	displayResult := ctx.ToDisplayResult()
	return r.Render(displayResult)
}
