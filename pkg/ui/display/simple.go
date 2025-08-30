package display

import (
	"fmt"
	"io"
	"sort"

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

	// Sort packs alphabetically
	packs := make([]types.DisplayPack, len(result.Packs))
	copy(packs, result.Packs)
	sort.Slice(packs, func(i, j int) bool {
		return packs[i].Name < packs[j].Name
	})

	// Render each pack
	for _, pack := range packs {
		if err := r.renderPack(pack); err != nil {
			return err
		}
	}

	return nil
}

// renderPack renders a single pack
func (r *TextRenderer) renderPack(pack types.DisplayPack) error {
	// Pack header with status - include pack-level status for debugging
	packStatus := pack.Status
	if packStatus == "" {
		// Calculate status if not set
		packStatus = pack.GetPackStatus()
	}

	packHeader := fmt.Sprintf("%s [status=%s]", pack.Name, packStatus)
	if pack.IsIgnored {
		packHeader += " [ignored]"
	}
	if pack.HasConfig {
		packHeader += " [config]"
	}

	if _, err := fmt.Fprintf(r.writer, "\n    %s:\n", packHeader); err != nil {
		return err
	}

	// Handle ignored directories specially
	if pack.IsIgnored {
		if _, err := fmt.Fprintln(r.writer, "        .dodotignore : dodot is ignoring this dir"); err != nil {
			return err
		}
		return nil
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
	// handler : path : message
	// Add status indicators and file override markers

	handler := file.Handler
	filePath := file.Path
	message := file.Message

	// Add override marker (asterisk) if file is overridden
	if file.IsOverride {
		filePath = "*" + filePath
	}

	// Add status indicator to message
	statusMessage := fmt.Sprintf("%s [status=%s]", message, file.Status)

	// Add timestamp if available
	if file.LastExecuted != nil {
		statusMessage += fmt.Sprintf(" [executed=%s]", file.LastExecuted.Format("2006-01-02"))
	}

	// Use consistent spacing with left-aligned columns
	_, err := fmt.Fprintf(r.writer, "        %-12s : %-20s : %s\n",
		handler,
		filePath,
		statusMessage)
	return err
}

// RenderExecutionContext is a convenience method that transforms and renders an ExecutionContext
func (r *TextRenderer) RenderExecutionContext(ctx *types.ExecutionContext) error {
	if ctx == nil {
		return nil
	}

	displayResult := ctx.ToDisplayResult()
	return r.Render(displayResult)
}
