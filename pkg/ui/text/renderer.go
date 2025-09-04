// Package text provides plain text output without any styling
package text

import (
	"fmt"
	"github.com/arthur-debert/dodot/pkg/execution/context"
	"io"

	"github.com/arthur-debert/dodot/pkg/ui/converter"
	"github.com/arthur-debert/dodot/pkg/ui/display"
)

// Renderer provides plain text output without colors or styling
type Renderer struct {
	output         io.Writer
	legacyRenderer *display.TextRenderer
}

// New creates a new text renderer
func New(output io.Writer) (*Renderer, error) {
	return &Renderer{
		output:         output,
		legacyRenderer: display.NewTextRenderer(output),
	}, nil
}

// RenderResult renders any result type as plain text
func (r *Renderer) RenderResult(result interface{}) error {
	// For now, delegate to the legacy renderer based on type
	switch v := result.(type) {
	case *display.PackCommandResult:
		// Convert PackCommandResult to DisplayResult for rendering
		displayResult := &display.DisplayResult{
			Command:   v.Command,
			Packs:     v.Packs,
			DryRun:    v.DryRun,
			Timestamp: v.Timestamp,
		}

		// Render the optional message first
		if v.Message != "" {
			if _, err := fmt.Fprintln(r.output, v.Message); err != nil {
				return err
			}
			// Add a blank line between message and pack status
			if _, err := fmt.Fprintln(r.output); err != nil {
				return err
			}
		}
		// Then render the pack status
		return r.legacyRenderer.Render(displayResult)
	case *display.CommandResult:
		// Legacy CommandResult support
		// Render the optional message first
		if v.Message != "" {
			if _, err := fmt.Fprintln(r.output, v.Message); err != nil {
				return err
			}
			// Add a blank line between message and pack status
			if _, err := fmt.Fprintln(r.output); err != nil {
				return err
			}
		}
		// Then render the pack status
		if v.Result != nil {
			return r.legacyRenderer.Render(v.Result)
		}
		return nil
	case *context.ExecutionContext:
		// Convert to DisplayResult first
		displayResult := converter.ConvertToDisplay(v)
		return r.legacyRenderer.Render(displayResult)
	case *display.DisplayResult:
		return r.legacyRenderer.Render(v)
	default:
		// For unknown types, just print them
		_, err := fmt.Fprintf(r.output, "%+v\n", result)
		return err
	}
}

// RenderError renders an error as plain text
func (r *Renderer) RenderError(err error) error {
	_, err2 := fmt.Fprintf(r.output, "Error: %v\n", err)
	return err2
}

// RenderMessage renders a simple message
func (r *Renderer) RenderMessage(msg string) error {
	_, err := fmt.Fprintln(r.output, msg)
	return err
}
